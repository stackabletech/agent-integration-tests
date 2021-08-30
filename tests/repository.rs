mod util;

use anyhow::Result;
use integration_test_commons::test::prelude::*;
use uuid::Uuid;

use crate::util::{
    repository::{StackableRepository, StackableRepositoryInstance},
    result::TestResult,
    services::noop_service,
};

#[tokio::test]
async fn invalid_or_unreachable_repositories_should_be_ignored() -> Result<()> {
    let client = KubeClient::new().await?;

    let mut result = TestResult::default();

    // Set up repositories and pod

    // The agent processes the repositories by their name in
    // alphabetical order.

    let repository0_result = client
        .create::<Repository>(indoc!(
            "
            apiVersion: stable.stackable.de/v1
            kind: Repository
            metadata:
                name: 0-no-repository-url
                namespace: default
            spec:
                repo_type: StackableRepo
                properties: {}
            "
        ))
        .await;
    result.combine(&repository0_result);

    let repository1_result = client
        .create::<Repository>(indoc!(
            "
            apiVersion: stable.stackable.de/v1
            kind: Repository
            metadata:
                name: 1-unreachable
                namespace: default
            spec:
                repo_type: StackableRepo
                properties:
                    url: https://unreachable
            "
        ))
        .await;
    result.combine(&repository1_result);

    let empty_repository = StackableRepository {
        name: String::from("2-empty-repository"),
        packages: Vec::new(),
    };
    let repository2_result = StackableRepositoryInstance::new(&empty_repository, &client).await;
    result.combine(&repository2_result);

    let mut service = noop_service();
    // Add a UUID to the service name to circumvent the package cache
    service.name.push_str(&format!("-{}", Uuid::new_v4()));

    let repository_with_service = StackableRepository {
        name: String::from("3-repository-with-service"),
        packages: vec![service.clone()],
    };
    let repository3_result =
        StackableRepositoryInstance::new(&repository_with_service, &client).await;
    result.combine(&repository3_result);

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
    if let Ok(repository3) = repository3_result {
        let close_result = repository3.close(&client).await;
        result.combine(&close_result);
    }
    if let Ok(repository2) = repository2_result {
        let close_result = repository2.close(&client).await;
        result.combine(&close_result);
    }
    if let Ok(repository1) = repository1_result {
        let close_result = client.delete(repository1).await;
        result.combine(&close_result);
    }
    if let Ok(repository0) = repository0_result {
        let close_result = client.delete(repository0).await;
        result.combine(&close_result);
    }

    // Return test result

    result.into()
}
