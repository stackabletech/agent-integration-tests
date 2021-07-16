mod util;

use std::fmt::Debug;

use anyhow::{anyhow, Result};
use integration_test_commons::test::prelude::*;
use uuid::Uuid;

use crate::util::{
    repository::{StackableRepository, StackableRepositoryInstance},
    services::noop_service,
};

#[tokio::test]
async fn invalid_or_unreachable_repositories_should_be_ignored() -> Result<()> {
    let client = KubeClient::new().await?;

    let mut result = Ok(());

    // Set up repositories and pod

    // The agent processes the repositories by their name in
    // alphabetical order.

    let repository0_result = client
        .create::<Repository>(
            "
                apiVersion: stable.stackable.de/v1
                kind: Repository
                metadata:
                    name: 0-no-repository-url
                    namespace: default
                spec:
                    repo_type: StackableRepo
                    properties: {}
            ",
        )
        .await;
    combine(&mut result, &repository0_result);

    let repository1_result = client
        .create::<Repository>(
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
            ",
        )
        .await;
    combine(&mut result, &repository1_result);

    let empty_repository = StackableRepository {
        name: String::from("2-empty-repository"),
        packages: Vec::new(),
    };
    let repository2_result = StackableRepositoryInstance::new(&empty_repository, &client).await;
    combine(&mut result, &repository2_result);

    let mut service = noop_service();
    // Add a UUID to the service name to circumvent the package cache
    service.name.push_str(&format!("-{}", Uuid::new_v4()));

    let repository_with_service = StackableRepository {
        name: String::from("3-repository-with-service"),
        packages: vec![service.clone()],
    };
    let repository3_result =
        StackableRepositoryInstance::new(&repository_with_service, &client).await;
    combine(&mut result, &repository3_result);

    let pod_result = client
        .create::<Pod>(&service.pod_spec("agent-service-integration-test-repository"))
        .await;
    combine(&mut result, &pod_result);

    // Verify that the pod was downloaded, started, and is ready

    if let Ok(pod) = &pod_result {
        let pod_ready = client.verify_pod_condition(&pod, "Ready").await;
        combine(&mut result, &pod_ready);
    }

    // Tear down pod and repositories

    if let Ok(pod) = pod_result {
        let deletion_result = client.delete(pod).await;
        combine(&mut result, &deletion_result);
    }
    if let Ok(repository3) = repository3_result {
        let close_result = repository3.close(&client).await;
        combine(&mut result, &close_result);
    }
    if let Ok(repository2) = repository2_result {
        let close_result = repository2.close(&client).await;
        combine(&mut result, &close_result);
    }
    if let Ok(repository1) = repository1_result {
        let close_result = client.delete(repository1).await;
        combine(&mut result, &close_result);
    }
    if let Ok(repository0) = repository0_result {
        let close_result = client.delete(repository0).await;
        combine(&mut result, &close_result);
    }

    // Return test result

    result
}

/// Applies the AND operation to the given results
///
/// If `result` contains already an error then `other_result` is
/// ignored else if `other_result` contains an error then it is applied
/// on `result`.
fn combine<T, E>(result: &mut Result<()>, other_result: &Result<T, E>)
where
    E: Debug,
{
    if result.is_ok() {
        if let Err(error) = other_result {
            *result = Err(anyhow!("{:?}", error))
        }
    }
}
