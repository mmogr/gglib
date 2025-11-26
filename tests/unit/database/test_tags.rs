//! Unit tests for tag operations.

use gglib::services::database::{
    add_model, add_model_tag, get_model_tags, get_models_by_tag, list_models, list_tags,
    remove_model_tag,
};

#[path = "../../common/mod.rs"]
mod common;

use common::database::setup_test_pool;
use common::fixtures::create_test_model;

#[tokio::test]
async fn test_add_model_tag() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("tag_test");
    add_model(&pool, &model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    add_model_tag(&pool, model_id, "favorite".to_string())
        .await
        .unwrap();

    let tags = get_model_tags(&pool, model_id).await.unwrap();
    assert_eq!(tags, vec!["favorite"]);
}

#[tokio::test]
async fn test_add_duplicate_tag_is_noop() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("dup_tag_test");
    add_model(&pool, &model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    add_model_tag(&pool, model_id, "favorite".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, model_id, "favorite".to_string())
        .await
        .unwrap();

    let tags = get_model_tags(&pool, model_id).await.unwrap();
    assert_eq!(tags, vec!["favorite"]);
}

#[tokio::test]
async fn test_remove_model_tag() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("remove_tag_test");
    add_model(&pool, &model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    add_model_tag(&pool, model_id, "favorite".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, model_id, "fast".to_string())
        .await
        .unwrap();

    remove_model_tag(&pool, model_id, "favorite".to_string())
        .await
        .unwrap();

    let tags = get_model_tags(&pool, model_id).await.unwrap();
    assert_eq!(tags, vec!["fast"]);
}

#[tokio::test]
async fn test_remove_nonexistent_tag_is_noop() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("remove_missing_tag");
    add_model(&pool, &model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    // Should not error when removing a tag that doesn't exist
    let result = remove_model_tag(&pool, model_id, "nonexistent".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_tags() {
    let pool = setup_test_pool().await.unwrap();

    let model1 = create_test_model("model1");
    let model2 = create_test_model("model2");
    add_model(&pool, &model1).await.unwrap();
    add_model(&pool, &model2).await.unwrap();

    let models = list_models(&pool).await.unwrap();

    add_model_tag(&pool, models[0].id.unwrap(), "fast".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, models[0].id.unwrap(), "coding".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, models[1].id.unwrap(), "fast".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, models[1].id.unwrap(), "chat".to_string())
        .await
        .unwrap();

    let all_tags = list_tags(&pool).await.unwrap();
    // Tags should be sorted and unique
    assert_eq!(all_tags, vec!["chat", "coding", "fast"]);
}

#[tokio::test]
async fn test_get_models_by_tag() {
    let pool = setup_test_pool().await.unwrap();

    let model1 = create_test_model("model1");
    let model2 = create_test_model("model2");
    let model3 = create_test_model("model3");
    add_model(&pool, &model1).await.unwrap();
    add_model(&pool, &model2).await.unwrap();
    add_model(&pool, &model3).await.unwrap();

    let models = list_models(&pool).await.unwrap();

    add_model_tag(&pool, models[0].id.unwrap(), "fast".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, models[1].id.unwrap(), "fast".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, models[2].id.unwrap(), "slow".to_string())
        .await
        .unwrap();

    let fast_models = get_models_by_tag(&pool, "fast".to_string()).await.unwrap();
    assert_eq!(fast_models.len(), 2);

    let slow_models = get_models_by_tag(&pool, "slow".to_string()).await.unwrap();
    assert_eq!(slow_models.len(), 1);
}

#[tokio::test]
async fn test_tags_are_sorted() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("sort_test");
    add_model(&pool, &model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    // Add tags in non-alphabetical order
    add_model_tag(&pool, model_id, "zebra".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, model_id, "apple".to_string())
        .await
        .unwrap();
    add_model_tag(&pool, model_id, "mango".to_string())
        .await
        .unwrap();

    let tags = get_model_tags(&pool, model_id).await.unwrap();
    assert_eq!(tags, vec!["apple", "mango", "zebra"]);
}
