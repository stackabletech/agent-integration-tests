mod test;
use std::time::Duration;
use test::prelude::*;

#[test]
fn service_should_be_started_successfully() {
    let client = TestKubeClient::new();

    setup_repository(&client);

    let pod = TemporaryResource::new(
        &client,
        &with_unique_name(indoc! {"
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
        "}),
    );

    client.verify_pod_condition(&pod, "Ready");
}

#[test]
fn restart_after_ungraceful_shutdown_should_succeed() {
    // must be greater than the period between the deletion of the pod
    // and the creation of the new systemd service
    let termination_grace_period = Duration::from_secs(5);

    let mut client = TestKubeClient::new();
    // delete must await the end of the termination grace period
    client.timeouts().delete += termination_grace_period;

    setup_repository(&client);

    let pod_spec = with_unique_name(&formatdoc! {"
        apiVersion: v1
        kind: Pod
        metadata:
          name: agent-service-integration-test-restart
        spec:
          containers:
            - name: nostop-service
              image: nostop-service:1.0.1
              command:
                - nostop-service-1.0.1/start.sh
          tolerations:
            - key: kubernetes.io/arch
              operator: Equal
              value: stackable-linux
          terminationGracePeriodSeconds: {termination_grace_period_seconds}
    ", termination_grace_period_seconds = termination_grace_period.as_secs()});

    for _ in 1..=2 {
        let pod = TemporaryResource::new(&client, &pod_spec);
        client.verify_pod_condition(&pod, "Ready");
    }
}

// This test provokes race conditions but does not guarantee their
// absence on success.
#[test]
fn no_race_conditions_should_occur_if_many_pods_are_started_and_stopped_in_parallel() {
    let mut client = TestKubeClient::new();
    client.timeouts().verify_pod_condition = Duration::from_secs(120);

    setup_repository(&client);

    let pod_spec = indoc! {"
        apiVersion: v1
        kind: Pod
        metadata:
          name: agent-service-integration-test-race-condition
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
    "};

    let mut pods = Vec::new();

    for _ in 1..=100 {
        pods.push(TemporaryResource::new(&client, &with_unique_name(pod_spec)));
    }

    for pod in &pods {
        client.verify_pod_condition(&pod, "Ready");
    }

    pods.clear();
}
