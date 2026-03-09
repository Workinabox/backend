use wiab_app::SomeService;
use wiab_core::Spec;
use wiab_inf::SomePersistence;

#[tokio::main]
async fn main() {
    let service = SomeService {
        name: "SomeService".to_string(),
        version: "0.1.0".to_string(),
    };
    println!("Service: {} v{}", service.name, service.version);

    let spec = Spec {
        name: "Agent environment".to_string(),
        version: "0.1.0".to_string(),
    };
    println!("Spec: {} v{}", spec.name, spec.version);

    let persistence = SomePersistence {
        name: "SQL Server".to_string(),
        version: "0.1.0".to_string(),
    };
    println!("Persistence: {} v{}", persistence.name, persistence.version);
}
