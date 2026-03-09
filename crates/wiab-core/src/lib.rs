pub struct Spec {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let spec = Spec {
            name: "Agent environment".to_string(),
            version: "0.1.0".to_string(),
        };
        assert_eq!(spec.name, "Agent environment");
        assert_eq!(spec.version, "0.1.0");
    }
}
