use crate::user::UserId;

/// Port that mints the next sequential `U-###` identifier (infrastructure seam).
pub trait UserNumbering: Send + Sync {
    fn next(&self) -> UserId;
}
