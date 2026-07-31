use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use git2::{BranchType, Commit, ObjectType, Oid, Repository, Signature, Tree};
use wiab_core::repo::{
    BranchName, BranchSnapshot, CommitHash, CommitSnapshot, FileChange, FileEntrySnapshot,
    GitBackend, GitBackendError, RepoId,
};

/// `GitBackend` implementation backed by libgit2 over bare repositories on disk.
///
/// Every repo maps to `<root>/R-<n>.git`. Object reads and server-side commits go
/// through libgit2; the on-the-wire git protocol (clone/push) is served separately by
/// spawning the system `git` against these same bare repos.
pub struct Git2Backend {
    root: PathBuf,
}

impl Git2Backend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Deterministic on-disk location of a repo's bare git directory.
    pub fn path_for(&self, id: &RepoId) -> PathBuf {
        self.root.join(format!("{id}.git"))
    }

    fn open(&self, id: &RepoId) -> Result<Repository, GitBackendError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(GitBackendError::RepoNotFound);
        }
        Repository::open_bare(&path).map_err(backend)
    }
}

fn backend(error: git2::Error) -> GitBackendError {
    GitBackendError::Backend(error.to_string())
}

fn branch_commit<'r>(
    repo: &'r Repository,
    branch: &BranchName,
) -> Result<Commit<'r>, GitBackendError> {
    let refname = format!("refs/heads/{}", branch.as_str());
    let reference = repo
        .find_reference(&refname)
        .map_err(|_| GitBackendError::BranchNotFound(branch.as_str().to_owned()))?;
    reference.peel_to_commit().map_err(backend)
}

fn commit_snapshot(commit: &Commit) -> CommitSnapshot {
    let author = commit.author();
    CommitSnapshot {
        hash: commit.id().to_string(),
        message: commit.message().unwrap_or_default().to_owned(),
        author: format!(
            "{} <{}>",
            author.name().unwrap_or_default(),
            author.email().unwrap_or_default()
        ),
        time_unix: commit.time().seconds(),
        parents: commit.parent_ids().map(|oid| oid.to_string()).collect(),
    }
}

/// Recursively writes a tree applying `files` (path components → blob oid) on top of an
/// optional base tree, creating subtrees as needed. Works on bare repos, which have no
/// index to stage into.
fn write_tree(
    repo: &Repository,
    base: Option<Tree<'_>>,
    files: &[(Vec<String>, Oid)],
) -> Result<Oid, GitBackendError> {
    let mut builder = repo.treebuilder(base.as_ref()).map_err(backend)?;
    let mut subdirs: BTreeMap<String, Vec<(Vec<String>, Oid)>> = BTreeMap::new();

    for (components, oid) in files {
        match components.as_slice() {
            [name] => {
                builder.insert(name, *oid, 0o100644).map_err(backend)?;
            }
            [head, rest @ ..] => {
                subdirs
                    .entry(head.clone())
                    .or_default()
                    .push((rest.to_vec(), *oid));
            }
            [] => {}
        }
    }

    for (dir, group) in subdirs {
        let existing = match builder.get(&dir).map_err(backend)? {
            Some(entry) if entry.kind() == Some(ObjectType::Tree) => Some(entry.id()),
            _ => None,
        };
        let base_subtree = match existing {
            Some(oid) => Some(repo.find_tree(oid).map_err(backend)?),
            None => None,
        };
        let sub_oid = write_tree(repo, base_subtree, &group)?;
        builder.insert(&dir, sub_oid, 0o040000).map_err(backend)?;
    }

    builder.write().map_err(backend)
}

impl GitBackend for Git2Backend {
    fn init_bare(&self, id: &RepoId) -> Result<(), GitBackendError> {
        let path = self.path_for(id);
        if path.exists() {
            return Ok(());
        }
        let repo = Repository::init_bare(&path).map_err(backend)?;
        // Point HEAD at `main` so the first push lands on the expected default branch.
        repo.set_head("refs/heads/main").map_err(backend)?;
        Ok(())
    }

    fn branches(&self, id: &RepoId) -> Result<Vec<BranchSnapshot>, GitBackendError> {
        let repo = self.open(id)?;
        let mut out = Vec::new();
        for entry in repo.branches(Some(BranchType::Local)).map_err(backend)? {
            let (branch, _) = entry.map_err(backend)?;
            let name = branch
                .name()
                .map_err(backend)?
                .unwrap_or_default()
                .to_owned();
            let target = branch
                .get()
                .target()
                .map(|oid| oid.to_string())
                .unwrap_or_default();
            out.push(BranchSnapshot { name, target });
        }
        Ok(out)
    }

    fn list_files(
        &self,
        id: &RepoId,
        branch: &BranchName,
        dir: &str,
    ) -> Result<Vec<FileEntrySnapshot>, GitBackendError> {
        let repo = self.open(id)?;
        let commit = branch_commit(&repo, branch)?;
        let root = commit.tree().map_err(backend)?;

        let (tree, prefix) = if dir.is_empty() {
            (root, String::new())
        } else {
            let entry = root
                .get_path(Path::new(dir))
                .map_err(|_| GitBackendError::PathNotFound(dir.to_owned()))?;
            let object = entry.to_object(&repo).map_err(backend)?;
            let subtree = object
                .into_tree()
                .map_err(|_| GitBackendError::PathNotFound(dir.to_owned()))?;
            (subtree, format!("{}/", dir.trim_end_matches('/')))
        };

        let mut out = Vec::new();
        for entry in tree.iter() {
            let name = entry.name().unwrap_or_default();
            out.push(FileEntrySnapshot {
                path: format!("{prefix}{name}"),
                is_dir: entry.kind() == Some(ObjectType::Tree),
            });
        }
        Ok(out)
    }

    fn read_file(
        &self,
        id: &RepoId,
        branch: &BranchName,
        path: &str,
    ) -> Result<Vec<u8>, GitBackendError> {
        let repo = self.open(id)?;
        let commit = branch_commit(&repo, branch)?;
        let tree = commit.tree().map_err(backend)?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| GitBackendError::PathNotFound(path.to_owned()))?;
        let object = entry.to_object(&repo).map_err(backend)?;
        let blob = object
            .as_blob()
            .ok_or_else(|| GitBackendError::NotAFile(path.to_owned()))?;
        Ok(blob.content().to_vec())
    }

    fn recent_commits(
        &self,
        id: &RepoId,
        branch: &BranchName,
        limit: usize,
    ) -> Result<Vec<CommitSnapshot>, GitBackendError> {
        let repo = self.open(id)?;
        let tip = branch_commit(&repo, branch)?;
        let mut walk = repo.revwalk().map_err(backend)?;
        walk.push(tip.id()).map_err(backend)?;
        let mut out = Vec::new();
        for oid in walk {
            if out.len() >= limit {
                break;
            }
            let oid = oid.map_err(backend)?;
            let commit = repo.find_commit(oid).map_err(backend)?;
            out.push(commit_snapshot(&commit));
        }
        Ok(out)
    }

    fn commit_changes(
        &self,
        id: &RepoId,
        branch: &BranchName,
        author_name: &str,
        author_email: &str,
        message: &str,
        changes: Vec<FileChange>,
    ) -> Result<CommitHash, GitBackendError> {
        let repo = self.open(id)?;
        let refname = format!("refs/heads/{}", branch.as_str());

        let parent = match repo.find_reference(&refname) {
            Ok(reference) => Some(reference.peel_to_commit().map_err(backend)?),
            Err(_) => None,
        };
        let base_tree = match &parent {
            Some(commit) => Some(commit.tree().map_err(backend)?),
            None => None,
        };

        let files = changes
            .into_iter()
            .map(|change| {
                let oid = repo.blob(&change.content).map_err(backend)?;
                let components = change
                    .path
                    .split('/')
                    .filter(|c| !c.is_empty())
                    .map(|c| c.to_owned())
                    .collect::<Vec<_>>();
                Ok((components, oid))
            })
            .collect::<Result<Vec<_>, GitBackendError>>()?;

        let tree_oid = write_tree(&repo, base_tree, &files)?;
        let tree = repo.find_tree(tree_oid).map_err(backend)?;

        let signature = Signature::now(author_name, author_email).map_err(backend)?;
        let parents: Vec<&Commit> = parent.iter().collect();
        let commit_oid = repo
            .commit(
                Some(&refname),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .map_err(backend)?;

        CommitHash::new(commit_oid.to_string())
            .map_err(|_| GitBackendError::Backend("git produced an invalid commit hash".to_owned()))
    }

    fn merge_branch(
        &self,
        id: &RepoId,
        source: &BranchName,
        target: &BranchName,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<CommitHash, GitBackendError> {
        let repo = self.open(id)?;
        let source_commit = branch_commit(&repo, source)?;
        let target_commit = branch_commit(&repo, target)?;

        // Already integrated: nothing to merge, and a merge commit here would be noise.
        // Report the target's existing tip so the caller records a real hash.
        if repo
            .graph_descendant_of(target_commit.id(), source_commit.id())
            .map_err(backend)?
        {
            return CommitHash::new(target_commit.id().to_string()).map_err(|_| {
                GitBackendError::Backend("git produced an invalid commit hash".to_owned())
            });
        }

        // merge_commits does the three-way merge in memory, so a bare repo needs no
        // worktree and a conflict leaves nothing on disk to clean up.
        let mut index = repo
            .merge_commits(&target_commit, &source_commit, None)
            .map_err(backend)?;
        if index.has_conflicts() {
            return Err(GitBackendError::MergeConflict {
                source_branch: source.as_str().to_owned(),
                target_branch: target.as_str().to_owned(),
            });
        }

        let tree_oid = index.write_tree_to(&repo).map_err(backend)?;
        let tree = repo.find_tree(tree_oid).map_err(backend)?;
        let signature = Signature::now(author_name, author_email).map_err(backend)?;
        // Always a merge commit, even where a fast-forward would do: one code path, and
        // the history explicitly records that the change arrived via a pull request.
        let commit_oid = repo
            .commit(
                Some(&format!("refs/heads/{}", target.as_str())),
                &signature,
                &signature,
                message,
                &tree,
                &[&target_commit, &source_commit],
            )
            .map_err(backend)?;

        CommitHash::new(commit_oid.to_string())
            .map_err(|_| GitBackendError::Backend("git produced an invalid commit hash".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn backend_with_repo() -> (TempDir, Git2Backend, RepoId) {
        let dir = TempDir::new().unwrap();
        let backend = Git2Backend::new(dir.path());
        let id = RepoId::from_number(7);
        backend.init_bare(&id).unwrap();
        (dir, backend, id)
    }

    fn main_branch() -> BranchName {
        BranchName::new("main").unwrap()
    }

    fn change(path: &str, content: &[u8]) -> FileChange {
        FileChange {
            path: path.to_owned(),
            content: content.to_vec(),
        }
    }

    fn branch(name: &str) -> BranchName {
        BranchName::new(name).unwrap()
    }

    /// A repo with `main` seeded, plus a `feature` branch cut from it, each having
    /// changed a different file — so the two merge cleanly.
    fn backend_with_divergent_branches() -> (TempDir, Git2Backend, RepoId) {
        let (dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "seed",
                vec![change("README.md", b"seed")],
            )
            .unwrap();
        // Cut `feature` from main's tip by committing onto it from main's tree.
        let repo = Repository::open_bare(dir.path().join("R-7.git")).unwrap();
        let tip = repo.find_reference("refs/heads/main").unwrap();
        let tip = tip.peel_to_commit().unwrap();
        repo.reference("refs/heads/feature", tip.id(), true, "cut")
            .unwrap();
        backend
            .commit_changes(
                &id,
                &branch("feature"),
                "Grace",
                "grace@example.com",
                "feature work",
                vec![change("feature.txt", b"from feature")],
            )
            .unwrap();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "main work",
                vec![change("main.txt", b"from main")],
            )
            .unwrap();
        (dir, backend, id)
    }

    #[test]
    fn merge_branch_creates_a_two_parent_commit_and_moves_the_target() {
        let (dir, backend, id) = backend_with_divergent_branches();
        let merged = backend
            .merge_branch(
                &id,
                &branch("feature"),
                &main_branch(),
                "Ada",
                "ada@example.com",
                "Merge PR-1",
            )
            .unwrap();

        let repo = Repository::open_bare(dir.path().join("R-7.git")).unwrap();
        let tip = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        assert_eq!(tip.id().to_string(), merged.as_str());
        assert_eq!(tip.parent_count(), 2);

        // Both sides' files are present at the merged tip.
        for path in ["main.txt", "feature.txt"] {
            assert!(
                backend.read_file(&id, &main_branch(), path).is_ok(),
                "{path} should be present after the merge"
            );
        }
    }

    #[test]
    fn merge_branch_reports_a_conflict_and_leaves_the_target_untouched() {
        let (dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "seed",
                vec![change("shared.txt", b"base")],
            )
            .unwrap();
        let repo = Repository::open_bare(dir.path().join("R-7.git")).unwrap();
        let tip = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        repo.reference("refs/heads/feature", tip.id(), true, "cut")
            .unwrap();
        // Both branches change the same file differently.
        backend
            .commit_changes(
                &id,
                &branch("feature"),
                "Grace",
                "grace@example.com",
                "feature edit",
                vec![change("shared.txt", b"from feature")],
            )
            .unwrap();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "main edit",
                vec![change("shared.txt", b"from main")],
            )
            .unwrap();
        let before = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        let error = backend
            .merge_branch(
                &id,
                &branch("feature"),
                &main_branch(),
                "Ada",
                "ada@example.com",
                "Merge PR-1",
            )
            .unwrap_err();

        assert_eq!(
            error,
            GitBackendError::MergeConflict {
                source_branch: "feature".to_owned(),
                target_branch: "main".to_owned(),
            }
        );
        let after = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "a conflict must not move the target ref");
        assert_eq!(
            backend
                .read_file(&id, &main_branch(), "shared.txt")
                .unwrap(),
            b"from main".to_vec()
        );
    }

    #[test]
    fn merging_an_already_integrated_branch_adds_no_commit() {
        let (dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "seed",
                vec![change("README.md", b"seed")],
            )
            .unwrap();
        let repo = Repository::open_bare(dir.path().join("R-7.git")).unwrap();
        let tip = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        repo.reference("refs/heads/stale", tip.id(), true, "cut")
            .unwrap();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "moves on",
                vec![change("later.txt", b"later")],
            )
            .unwrap();
        let before = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        let merged = backend
            .merge_branch(
                &id,
                &branch("stale"),
                &main_branch(),
                "Ada",
                "ada@example.com",
                "Merge PR-1",
            )
            .unwrap();

        assert_eq!(merged.as_str(), before.to_string());
    }

    #[test]
    fn merge_branch_rejects_an_unknown_branch() {
        let (_dir, backend, id) = backend_with_divergent_branches();
        let error = backend
            .merge_branch(
                &id,
                &branch("nope"),
                &main_branch(),
                "Ada",
                "ada@example.com",
                "Merge",
            )
            .unwrap_err();
        assert_eq!(error, GitBackendError::BranchNotFound("nope".to_owned()));
    }

    #[test]
    fn init_bare_is_idempotent_and_creates_the_directory() {
        let (dir, backend, id) = backend_with_repo();
        assert!(dir.path().join("R-7.git").exists());
        // A second call must not fail or wipe anything.
        backend.init_bare(&id).unwrap();
    }

    #[test]
    fn commit_then_read_round_trips_nested_paths() {
        let (_dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@example.com",
                "initial",
                vec![
                    change("README.md", b"hello"),
                    change("src/main.rs", b"fn main() {}"),
                ],
            )
            .unwrap();

        assert_eq!(
            backend.read_file(&id, &main_branch(), "README.md").unwrap(),
            b"hello"
        );
        assert_eq!(
            backend
                .read_file(&id, &main_branch(), "src/main.rs")
                .unwrap(),
            b"fn main() {}"
        );
    }

    #[test]
    fn second_commit_builds_on_the_first() {
        let (_dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@x.com",
                "one",
                vec![change("a.txt", b"1")],
            )
            .unwrap();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@x.com",
                "two",
                vec![change("b.txt", b"2")],
            )
            .unwrap();

        // First file survives the second commit.
        assert_eq!(
            backend.read_file(&id, &main_branch(), "a.txt").unwrap(),
            b"1"
        );
        let commits = backend.recent_commits(&id, &main_branch(), 10).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "two");
        assert_eq!(commits[1].message, "one");
        assert_eq!(commits[0].parents, vec![commits[1].hash.clone()]);
    }

    #[test]
    fn branches_lists_the_default_branch_after_a_commit() {
        let (_dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@x.com",
                "init",
                vec![change("a", b"x")],
            )
            .unwrap();
        let branches = backend.branches(&id).unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
    }

    #[test]
    fn list_files_returns_root_entries() {
        let (_dir, backend, id) = backend_with_repo();
        backend
            .commit_changes(
                &id,
                &main_branch(),
                "Ada",
                "ada@x.com",
                "init",
                vec![change("README.md", b"x"), change("src/main.rs", b"y")],
            )
            .unwrap();
        let mut entries = backend.list_files(&id, &main_branch(), "").unwrap();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "README.md");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[1].path, "src");
        assert!(entries[1].is_dir);
    }

    #[test]
    fn missing_repo_and_branch_report_errors() {
        let (_dir, backend, _id) = backend_with_repo();
        let missing = RepoId::from_number(99);
        assert_eq!(
            backend.branches(&missing).unwrap_err(),
            GitBackendError::RepoNotFound
        );

        let (_dir2, backend2, id) = backend_with_repo();
        assert_eq!(
            backend2.read_file(&id, &main_branch(), "nope").unwrap_err(),
            GitBackendError::BranchNotFound("main".to_owned())
        );
    }
}
