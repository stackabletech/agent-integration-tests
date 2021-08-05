mod util;

use anyhow::Result;
use integration_test_commons::test::prelude::*;
use rstest::rstest;
use util::{
    repository::{StackableRepository, StackableRepositoryInstance},
    result::TestResult,
    services::exit_service,
};
use uuid::Uuid;

#[rstest]
#[case::failing_service_should_be_restarted_on_restart_policy_always(
    "failing_service",
    "Always",
    "expect_restart"
)]
#[case::failing_service_should_be_restarted_on_restart_policy_onfailure(
    "failing_service",
    "OnFailure",
    "expect_restart"
)]
#[case::failing_service_should_not_be_restarted_on_restart_policy_never(
    "failing_service",
    "Never",
    "expect_no_restart"
)]
#[case::succeeding_service_should_be_restarted_on_restart_policy_always(
    "succeeding_service",
    "Always",
    "expect_restart"
)]
#[case::succeeding_service_should_not_be_restarted_on_restart_policy_onfailure(
    "succeeding_service",
    "OnFailure",
    "expect_no_restart"
)]
#[case::succeeding_service_should_not_be_restarted_on_restart_policy_never(
    "succeeding_service",
    "Never",
    "expect_no_restart"
)]
#[tokio::test]
async fn service_should_be_restarted_according_to_the_restart_policy(
    #[case] service: &str,
    #[case] restart_policy: &str,
    #[case] expected_behavior: &str,
) -> Result<()> {
    let client = KubeClient::new().await?;
    let mut result = TestResult::default();

    let (repository_result, pod_result) = set_up(
        &client,
        &mut result,
        match service {
            "succeeding_service" => true,
            "failing_service" => false,
            other => panic!("invalid parameter: {}", other),
        },
        restart_policy,
    )
    .await;

    match expected_behavior {
        "expect_restart" => verify_restart(&client, &mut result, &pod_result).await,
        "expect_no_restart" => verify_no_restart(&client, &mut result, &pod_result).await,
        other => panic!("invalid parameter: {}", other),
    }

    tear_down(&client, &mut result, repository_result, pod_result).await;

    result.into()
}

async fn set_up(
    client: &KubeClient,
    result: &mut TestResult,
    succeeding: bool,
    restart_policy: &str,
) -> (Result<StackableRepositoryInstance>, Result<Pod>) {
    let service = exit_service(if succeeding { 0 } else { 1 });

    let repository = StackableRepository {
        name: format!("restart-test-repository-{}", Uuid::new_v4()),
        packages: vec![service.to_owned()],
    };
    let repository_result = StackableRepositoryInstance::new(&repository, client).await;
    result.combine(&repository_result);

    let mut pod_definition = service.pod(&format!(
        "agent-service-integration-test-restart-{}",
        Uuid::new_v4()
    ));
    pod_definition
        .spec
        .get_or_insert_with(Default::default)
        .restart_policy
        .replace(String::from(restart_policy));

    let pod_result = client
        .create(&serde_yaml::to_string(&pod_definition).unwrap())
        .await;
    result.combine(&pod_result);

    (repository_result, pod_result)
}

async fn tear_down(
    client: &KubeClient,
    result: &mut TestResult,
    repository_result: Result<StackableRepositoryInstance>,
    pod_result: Result<Pod>,
) {
    if let Ok(pod) = pod_result {
        let deletion_result = client.delete(pod).await;
        result.combine(&deletion_result);
    }
    if let Ok(repository) = repository_result {
        let close_result = repository.close(client).await;
        result.combine(&close_result);
    }
}

async fn verify_restart(client: &KubeClient, result: &mut TestResult, pod_result: &Result<Pod>) {
    if let Ok(pod) = &pod_result {
        let verify_status_result = client
            .verify_status(pod, |pod| {
                pod.status
                    .as_ref()
                    .and_then(|status| status.container_statuses.first())
                    .filter(|container_status| container_status.restart_count > 3)
                    .is_some()
            })
            .await;
        result.combine(&verify_status_result);
    }
}

async fn verify_no_restart(client: &KubeClient, result: &mut TestResult, pod_result: &Result<Pod>) {
    if let Ok(pod) = &pod_result {
        let verify_status_result = client
            .verify_status(pod, |pod| {
                let phase = pod.status.as_ref().and_then(|status| status.phase.as_ref());
                phase == Some(&String::from("Succeeded")) || phase == Some(&String::from("Failed"))
            })
            .await;
        result.combine(&verify_status_result);

        let get_status_result = client.get_status(pod).await;
        result.combine(&get_status_result);

        if let Ok(pod) = get_status_result {
            let restart_count_result = pod
                .status
                .as_ref()
                .and_then(|status| status.container_statuses.first())
                .filter(|container_status| container_status.restart_count == 0)
                .ok_or("Restart count is not 0.");
            result.combine(&restart_count_result);
        }
    }
}
