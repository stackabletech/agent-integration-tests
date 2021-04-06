mod test;
use test::prelude::*;

#[test]
fn service_should_be_started_successfully() {
    let client = TestKubeClient::new();

    create_repository(&client);

    // Remove pod if it still exists from a previous test run.
    if let Some(pod) = client.find::<Pod>("agent-integration-test") {
        client.delete(pod);
    };

    let pod = client.create(indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: agent-integration-test
        spec:
          containers:
            - name: test-service
              image: test-service:0.1.0
              command:
                - test-service-0.1.0/start.sh
          tolerations:
            - key: kubernetes.io/arch
              operator: Equal
              value: stackable-linux
    "#});

    client.verify_pod_condition(&pod, "Ready");

    client.delete(pod);
}

fn create_repository(client: &TestKubeClient) {
    client.apply_crd(&Repository::crd());

    client.apply::<Repository>(indoc!("
        apiVersion: stable.stackable.de/v1
        kind: Repository
        metadata:
            name: integration-test-repository
            namespace: default
        spec:
            repo_type: StackableRepo
            properties:
                url: https://raw.githubusercontent.com/stackabletech/integration-test-repo/6d784f1fb433123cb3b1d5cd7364a4553246d749/
    "));
}
