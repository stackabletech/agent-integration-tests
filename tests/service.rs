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
              name: agent-service-integration-test
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
