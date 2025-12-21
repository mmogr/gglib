# services

Application service layer implementing business logic and use cases.

## Purpose

This module contains the **application services** that orchestrate business logic by:
- Coordinating between multiple ports (repositories, external services)
- Implementing business rules and validation
- Managing transactions and consistency
- Emitting domain events

## Architecture Pattern

**Services = Use Case Orchestrators**

```text
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Services (this module)                   │  │
│  │  - Orchestrate business logic                         │  │
│  │  - Coordinate multiple ports                          │  │
│  │  - Emit domain events                                 │  │
│  └─────────────┬─────────────────────────┬───────────────┘  │
│                │ uses                    │ uses             │
│                ▼                         ▼                  │
│  ┌──────────────────────┐    ┌──────────────────────┐      │
│  │   Domain Types       │    │   Port Traits        │      │
│  │   (../domain/)       │    │   (../ports/)        │      │
│  └──────────────────────┘    └──────────────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

## Service Organization

### Core Application Service
- **`app_core.rs`** - Central application coordinator
  - Wires together all services
  - Manages application lifecycle
  - Coordinates cross-cutting concerns

### Domain Services

#### Model Management
- **`model_service.rs`** - Model lifecycle operations
  - Add/remove models
  - List and filter models
  - Model validation
  - Integration with model catalog

- **`model_registrar.rs`** - Model registration and discovery
  - Register models from various sources
  - Update model metadata
  - Sync with external catalogs

#### Server Management
- **`server_service.rs`** - Server lifecycle management
  - Start/stop model servers
  - Health monitoring
  - Port allocation
  - Process supervision

#### Persistence Services
- **`chat_history.rs`** - Conversation management
  - Save chat conversations
  - Retrieve conversation history
  - Delete conversations
  - Search conversations

- **`settings_service.rs`** - Application settings
  - Get/update settings
  - Validation
  - Persistence

## Design Patterns

### Constructor Injection
```rust
pub struct ModelService<R: ModelRepository> {
    repo: Arc<R>,
    catalog: Arc<dyn ModelCatalog>,
    events: Arc<dyn AppEventEmitter>,
}

impl<R: ModelRepository> ModelService<R> {
    pub fn new(
        repo: Arc<R>,
        catalog: Arc<dyn ModelCatalog>,
        events: Arc<dyn AppEventEmitter>,
    ) -> Self {
        Self { repo, catalog, events }
    }
}
```

### Event Emission
Services emit events after state changes:
```rust
async fn add_model(&self, model: Model) -> Result<()> {
    self.repo.insert(&model).await?;
    self.events.emit(AppEvent::ModelAdded(model.id)).await;
    Ok(())
}
```

### Error Handling
Services convert port errors to domain errors:
```rust
async fn get_model(&self, id: &str) -> Result<Model, ServiceError> {
    self.repo.get(id)
        .await
        .map_err(ServiceError::Repository)
}
```

## Testing

Services are highly testable using mock ports:
```rust
#[cfg(test)]
mod tests {
    use mockall::mock;
    
    mock! {
        ModelRepo {}
        impl ModelRepository for ModelRepo {
            async fn list(&self) -> Result<Vec<Model>>;
        }
    }
    
    #[tokio::test]
    async fn test_list_models() {
        let mut mock = MockModelRepo::new();
        mock.expect_list().returning(|| Ok(vec![]));
        
        let service = ModelService::new(Arc::new(mock), ...);
        let result = service.list_models().await;
        assert!(result.is_ok());
    }
}
```

## Dependencies

Services depend on:
- **Domain types**: `../domain/` for entities
- **Port traits**: `../ports/` for infrastructure interfaces
- **Events**: `../events/` for event types
- **Standard library**: `std::sync::Arc` for shared ownership

## Usage Example

```rust
use gglib_core::services::ModelService;
use gglib_core::domain::Model;

async fn example(service: &ModelService<impl ModelRepository>) {
    let model = Model::new("my-model", "path/to/model.gguf");
    service.add_model(model).await?;
    
    let models = service.list_models().await?;
    println!("Total models: {}", models.len());
}
```

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`app_core.rs`](app_core) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-app_core-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-app_core-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-app_core-coverage.json) |
| [`chat_history.rs`](chat_history) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-chat_history-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-chat_history-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-chat_history-coverage.json) |
| [`model_registrar.rs`](model_registrar) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-model_registrar-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-model_registrar-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-model_registrar-coverage.json) |
| [`model_service.rs`](model_service) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-model_service-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-model_service-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-model_service-coverage.json) |
| [`server_service.rs`](server_service) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-server_service-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-server_service-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-server_service-coverage.json) |
| [`settings_service.rs`](settings_service) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-settings_service-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-settings_service-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-settings_service-coverage.json) |
<!-- module-table:end -->
