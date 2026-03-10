use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("room id cannot be empty")]
    EmptyRoomId,
    #[error("room capacity must be greater than 0, got {0}")]
    InvalidCapacity(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoomId(String);

impl RoomId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let normalized = value.trim();
        if normalized.is_empty() {
            return Err(DomainError::EmptyRoomId);
        }
        Ok(Self(normalized.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    id: RoomId,
    capacity: usize,
}

impl Room {
    pub fn new(id: RoomId, capacity: usize) -> Result<Self, DomainError> {
        if capacity == 0 {
            return Err(DomainError::InvalidCapacity(capacity));
        }
        Ok(Self { id, capacity })
    }

    pub fn id(&self) -> &RoomId {
        &self.id
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

pub trait RoomRepository {
    fn save(&mut self, room: Room);
    fn get(&self, id: &RoomId) -> Option<Room>;
    fn list(&self) -> Vec<Room>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_id_new_valid() {
        let id = RoomId::new("lobby").unwrap();
        assert_eq!(id.as_str(), "lobby");
    }

    #[test]
    fn room_id_new_trims_whitespace() {
        let id = RoomId::new("  lobby  ").unwrap();
        assert_eq!(id.as_str(), "lobby");
    }

    #[test]
    fn room_id_new_empty_string_errors() {
        let err = RoomId::new("").unwrap_err();
        assert_eq!(err, DomainError::EmptyRoomId);
    }

    #[test]
    fn room_id_new_whitespace_only_errors() {
        let err = RoomId::new("   ").unwrap_err();
        assert_eq!(err, DomainError::EmptyRoomId);
    }

    #[test]
    fn room_new_valid() {
        let id = RoomId::new("lobby").unwrap();
        let room = Room::new(id.clone(), 10).unwrap();
        assert_eq!(room.id(), &id);
        assert_eq!(room.capacity(), 10);
    }

    #[test]
    fn room_new_zero_capacity_errors() {
        let id = RoomId::new("lobby").unwrap();
        let err = Room::new(id, 0).unwrap_err();
        assert_eq!(err, DomainError::InvalidCapacity(0));
    }

    #[test]
    fn domain_error_display_empty_room_id() {
        assert_eq!(
            DomainError::EmptyRoomId.to_string(),
            "room id cannot be empty"
        );
    }

    #[test]
    fn domain_error_display_invalid_capacity() {
        assert_eq!(
            DomainError::InvalidCapacity(0).to_string(),
            "room capacity must be greater than 0, got 0"
        );
    }

    #[test]
    fn room_id_clone_and_eq() {
        let id = RoomId::new("a").unwrap();
        assert_eq!(id.clone(), id);
    }

    #[test]
    fn room_clone_and_eq() {
        let id = RoomId::new("a").unwrap();
        let room = Room::new(id, 5).unwrap();
        assert_eq!(room.clone(), room);
    }

    #[test]
    fn domain_error_clone_and_eq() {
        assert_eq!(DomainError::EmptyRoomId.clone(), DomainError::EmptyRoomId);
        assert_eq!(
            DomainError::InvalidCapacity(3).clone(),
            DomainError::InvalidCapacity(3)
        );
    }
}
