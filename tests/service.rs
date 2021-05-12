mod test;
use test::prelude::*;

#[test]
fn service_should_be_started_successfully() {
    let client = TestKubeClient::new();

    setup_repository(&client);

    let pod = TemporaryResource::new(
        &client,
        indoc! {"
            apiVersion: v1
            kind: Pod
            metadata:
              name: agent-service-integration-test-start
            spec:
              containers:
                - name: noop-service
                  image: noop-service:1.0.0
                  command:
                    - noop-service-1.0.0/start.sh
              tolerations:
                - key: kubernetes.io/arch
                  operator: Equal
                  value: stackable-linux
        "},
    );

    client.verify_pod_condition(&pod, "Ready");
}

#[test]
fn restart_after_ungraceful_shutdown_should_succeed() {
    let client = TestKubeClient::new();

    setup_repository(&client);

    let pod_spec = indoc! {"
        apiVersion: v1
        kind: Pod
        metadata:
          name: agent-service-integration-test-restart
        spec:
          containers:
            - name: nostop-service
              image: nostop-service:1.0.0
              command:
                - nostop-service-1.0.0/start.sh
          tolerations:
            - key: kubernetes.io/arch
              operator: Equal
              value: stackable-linux
    "};

    for _ in 1..=2 {
        let pod = TemporaryResource::new(&client, pod_spec);
        client.verify_pod_condition(&pod, "Ready");
    }
}
