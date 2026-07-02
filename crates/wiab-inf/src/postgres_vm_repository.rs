use deadpool_postgres::Pool;
use wiab_core::organization::OrganizationId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::vm::{Vm, VmId, VmRepository, VmResources, VmState, VmTemplate};

/// PostgreSQL-backed vm repository. One row per aggregate in `vm`, guarded by an
/// optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresVmRepository {
    pool: Pool,
}

impl PostgresVmRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn repo_error<E: std::fmt::Display>(error: E) -> RepoError {
    RepoError::Backend(error.to_string())
}

fn save_error<E: std::fmt::Display>(error: E) -> SaveError {
    SaveError::Backend(error.to_string())
}

/// Rebuild a `Vm` from a row's columns: (organization_id, template, state, guest_ip, vcpus, mem_mib).
fn vm_from_columns(
    id: VmId,
    organization_id: String,
    template: String,
    state: String,
    guest_ip: Option<String>,
    vcpus: i64,
    mem_mib: i64,
) -> Result<Vm, RepoError> {
    let organization_id: OrganizationId = organization_id.parse().map_err(repo_error)?;
    let template = VmTemplate::new(template).map_err(repo_error)?;
    let state: VmState = state.parse().map_err(repo_error)?;
    let resources = VmResources::new(vcpus as u32, mem_mib as u32);
    Ok(Vm::from_parts(
        id,
        organization_id,
        template,
        resources,
        state,
        guest_ip,
    ))
}

impl VmRepository for PostgresVmRepository {
    async fn save(&self, vm: Vm, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = vm.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let organization_id = vm.organization_id().to_string();
        let template = vm.template().name().to_owned();
        let state = vm.state().to_string();
        let guest_ip = vm.guest_ip().map(|ip| ip.to_owned());
        let vcpus = vm.resources().vcpus() as i64;
        let mem_mib = vm.resources().mem_mib() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO vm \
                     (id, version, organization_id, template, state, guest_ip, vcpus, mem_mib) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &organization_id,
                        &template,
                        &state,
                        &guest_ip,
                        &vcpus,
                        &mem_mib,
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE vm SET version = $2, organization_id = $3, template = $4, \
                     state = $5, guest_ip = $6, vcpus = $7, mem_mib = $8 \
                     WHERE id = $1 AND version = $9",
                    &[
                        &id,
                        &next_version,
                        &organization_id,
                        &template,
                        &state,
                        &guest_ip,
                        &vcpus,
                        &mem_mib,
                        &(expected.value() as i64),
                    ],
                )
                .await
                .map_err(save_error)?
        };
        if rows == 0 {
            return Err(SaveError::Conflict);
        }
        Ok(next)
    }

    async fn get(&self, id: &VmId) -> Result<Option<(Vm, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, organization_id, template, state, guest_ip, vcpus, mem_mib \
                 FROM vm WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let vm = vm_from_columns(
                    *id,
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                )?;
                Ok(Some((vm, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Vm>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, organization_id, template, state, guest_ip, vcpus, mem_mib FROM vm",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: VmId = id.parse().map_err(repo_error)?;
                vm_from_columns(
                    id,
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                )
            })
            .collect()
    }
}
