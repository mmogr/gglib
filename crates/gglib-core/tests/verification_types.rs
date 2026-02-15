//! Integration tests for model verification functionality.
//!
//! Tests that verification types and services compile and are accessible.
//! Full integration testing is done through CLI and API test suites.

use gglib_core::services::{ModelVerificationService, OverallHealth, ShardHealth};

#[test]
fn test_shard_health_variants() {
    // Test that all ShardHealth variants can be created
    let healthy = ShardHealth::Healthy;
    let corrupt = ShardHealth::Corrupt {
        expected: "abc123".to_string(),
        actual: "def456".to_string(),
    };
    let missing = ShardHealth::Missing;
    let no_oid = ShardHealth::NoOid;
    
    // All variants should be creatable
    assert!(matches!(healthy, ShardHealth::Healthy));
    assert!(matches!(corrupt, ShardHealth::Corrupt { .. }));
    assert!(matches!(missing, ShardHealth::Missing));
    assert!(matches!(no_oid, ShardHealth::NoOid));
}

#[test]
fn test_overall_health_variants() {
    // Test OverallHealth enum variants
    let healthy = OverallHealth::Healthy;
    let unhealthy = OverallHealth::Unhealthy;
    let unverifiable = OverallHealth::Unverifiable;
    
    assert_eq!(format!("{healthy:?}"), "Healthy");
    assert_eq!(format!("{unhealthy:?}"), "Unhealthy");
    assert_eq!(format!("{unverifiable:?}"), "Unverifiable");
}

#[test]
fn test_verification_service_type_exists() {
    // This test simply verifies that ModelVerificationService is a valid type.
    // Full integration tests are performed in CLI and API test suites which
    // provide the complete application context with proper dependency injection.
    let _type_check: Option<ModelVerificationService> = None;
}
