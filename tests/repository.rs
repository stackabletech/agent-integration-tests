mod util;

use anyhow::Result;
use integration_test_commons::test::prelude::*;
use uuid::Uuid;

use crate::util::repository::StackableRepositoryBuilder;
use crate::util::result::TestResult;
use crate::util::services::noop_service;

#[tokio::test]
async fn invalid_or_unreachable_repositories_should_be_ignored() -> Result<()> {
    let client = KubeClient::new().await?;

    let mut result = TestResult::default();

    // Set up repositories and pod

    // The agent processes the repositories by their name in
    // alphabetical order.

    let repository_without_url_result = StackableRepositoryBuilder::new("0-no-repository-url")
        .uri(&None)
        .run(&client)
        .await;
    result.combine(&repository_without_url_result);

    let repository_with_unreachable_url_result = StackableRepositoryBuilder::new("1-unreachable")
        .uri(&Some(String::from("https://unreachable")))
        .run(&client)
        .await;
    result.combine(&repository_with_unreachable_url_result);

    let repository_without_packages_result = StackableRepositoryBuilder::new("2-empty-repository")
        .run(&client)
        .await;
    result.combine(&repository_without_packages_result);

    let mut service = noop_service();
    // Add a UUID to the service name to circumvent the package cache
    service.name.push_str(&format!("-{}", Uuid::new_v4()));

    let repository_with_service_result =
        StackableRepositoryBuilder::new("3-repository-with-service")
            .package(&service)
            .run(&client)
            .await;
    result.combine(&repository_with_service_result);

    let pod_definition = service.pod("agent-service-integration-test-repository");
    let pod_result = client
        .create::<Pod>(&serde_yaml::to_string(&pod_definition).unwrap())
        .await;
    result.combine(&pod_result);

    // Verify that the pod was downloaded, started, and is ready

    if let Ok(pod) = &pod_result {
        let pod_ready = client.verify_pod_condition(pod, "Ready").await;
        result.combine(&pod_ready);
    }

    // Tear down pod and repositories

    if let Ok(pod) = pod_result {
        let deletion_result = client.delete(pod).await;
        result.combine(&deletion_result);
    }
    if let Ok(repository_with_service) = repository_with_service_result {
        let close_result = repository_with_service.close(&client).await;
        result.combine(&close_result);
    }
    if let Ok(repository_without_packages) = repository_without_packages_result {
        let close_result = repository_without_packages.close(&client).await;
        result.combine(&close_result);
    }
    if let Ok(repository_with_unreachable_url) = repository_with_unreachable_url_result {
        let close_result = repository_with_unreachable_url.close(&client).await;
        result.combine(&close_result);
    }
    if let Ok(repository_without_url) = repository_without_url_result {
        let close_result = repository_without_url.close(&client).await;
        result.combine(&close_result);
    }

    // Return test result

    result.into()
}
