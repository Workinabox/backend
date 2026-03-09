pub struct SomePersistence {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let persistence = SomePersistence {
            name: "SQL Server".to_string(),
            version: "0.1.0".to_string(),
        };
        assert_eq!(persistence.name, "SQL Server");
        assert_eq!(persistence.version, "0.1.0");
    }
}
