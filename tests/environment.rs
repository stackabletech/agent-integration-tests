mod util;

use anyhow::Result;
use integration_test_commons::test::prelude::*;

use util::{
    repository::{StackableRepository, StackableRepositoryInstance},
    result::TestResult,
    test_package::TestPackage,
};

#[tokio::test]
async fn kubeconfig_should_be_set() -> Result<()> {
    let client = KubeClient::new().await?;

    let mut result = TestResult::default();

    // Set up repository and pod

    // The job terminates successfully if the content of the environment
    // variable KUBECONFIG is not empty.
    let job = TestPackage {
        name: String::from("kubeconfig-test-job"),
        version: String::from("1.0.0"),
        job: true,
        script: String::from(indoc!(
            r#"
            #!/bin/sh

            test -n "$KUBECONFIG"
            "#
        )),
    };

    let repository = StackableRepository {
        name: String::from("kubeconfig-test-repository"),
        packages: vec![job.to_owned()],
    };
    let repository_result = StackableRepositoryInstance::new(&repository, &client).await;
    result.combine(&repository_result);

    let pod_definition = job.pod("agent-service-integration-test-kubeconfig");
    let pod_result = client
        .create(&serde_yaml::to_string(&pod_definition).unwrap())
        .await;
    result.combine(&pod_result);

    // Verify that the job terminated successfully

    if let Ok(pod) = &pod_result {
        let job_result = client
            .verify_status::<Pod, _>(pod, |pod| {
                let phase = pod.status.as_ref().and_then(|status| status.phase.as_ref());
                phase == Some(&String::from("Succeeded"))
            })
            .await;
        result.combine(&job_result);
    }

    // Tear down pod and repository

    if let Ok(pod) = pod_result {
        let deletion_result = client.delete(pod).await;
        result.combine(&deletion_result);
    }
    if let Ok(repository) = repository_result {
        let close_result = repository.close(&client).await;
        result.combine(&close_result);
    }

    // Return test result

    result.into()
}
