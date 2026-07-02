use std::fmt;
use std::str::FromStr;

use crate::vm::VmError;

/// Human-readable microVM identifier, rendered "VM-7".
///
/// The number is minted by the `VmNumbering` seam at the application layer and passed into
/// `Vm::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmId(u64);

impl VmId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VM-{}", self.0)
    }
}

impl FromStr for VmId {
    type Err = VmError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("VM-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(VmId)
            .ok_or_else(|| VmError::InvalidVmId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_a_vm_prefix() {
        assert_eq!(VmId::from_number(7).to_string(), "VM-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(VmId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!("VM-42".parse::<VmId>().unwrap(), VmId::from_number(42));
    }

    #[test]
    fn round_trips_through_string() {
        let id = VmId::from_number(123);
        assert_eq!(id.to_string().parse::<VmId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<VmId>().unwrap_err(),
            VmError::InvalidVmId("42".to_owned())
        );
        assert_eq!(
            "VM-abc".parse::<VmId>().unwrap_err(),
            VmError::InvalidVmId("VM-abc".to_owned())
        );
        assert_eq!(
            "A-1".parse::<VmId>().unwrap_err(),
            VmError::InvalidVmId("A-1".to_owned())
        );
    }
}
