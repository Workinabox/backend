pub struct SomeService {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let service = SomeService {
            name: "SomeService".to_string(),
            version: "0.1.0".to_string(),
        };
        assert_eq!(service.name, "SomeService");
        assert_eq!(service.version, "0.1.0");
    }
}
