mod test;
use futures::future::join_all;
use std::{fmt, time::Duration};
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

#[tokio::test(flavor = "multi_thread")]
async fn starting_and_stopping_100_pods_simultaneously_should_succeed() {
    let mut client = KubeClient::new()
        .await
        .expect("Kubernetes client could not be created");
    client.timeouts.create = Duration::from_secs(60);
    client.timeouts.delete = Duration::from_secs(60);
    client.timeouts.verify_pod_condition = Duration::from_secs(60);

    setup_repository_async(&client)
        .await
        .expect("Repository could not be setup.");

    const NUM_PODS: u32 = 100;

    let max_pods = client
        .list_labeled::<Node>("kubernetes.io/arch=stackable-linux")
        .await
        .expect("List of Stackable nodes could not be retrieved")
        .iter()
        .map(get_allocatable_pods)
        .sum();

    assert!(
        NUM_PODS <= max_pods,
        "The test case tries to create {} pods but only {} pods are allocatable on the nodes.",
        NUM_PODS,
        max_pods
    );

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

    let pod_specs = (0..NUM_PODS)
        .map(|_| with_unique_name(pod_spec))
        .collect::<Vec<_>>();

    let (pods, creation_errors) =
        partition_results(join_all(pod_specs.iter().map(|spec| client.create::<Pod>(spec))).await);
    let pods_created = pods.len();

    let (ready_successes, ready_errors) = partition_results(
        join_all(
            pods.iter()
                .map(|pod| client.verify_pod_condition(pod, "Ready")),
        )
        .await,
    );
    let pods_ready = ready_successes.len();

    let (deletion_successes, deletion_errors) =
        partition_results(join_all(pods.into_iter().map(|pod| client.delete(pod))).await);
    let pods_deleted = deletion_successes.len();

    let mut errors = Vec::new();
    errors.extend(creation_errors);
    errors.extend(ready_errors);
    errors.extend(deletion_errors);

    if let Some(error) = errors.first() {
        panic!(
            "Pods: {created}/{total} created, {ready}/{created} ready, {deleted}/{created} deleted; Error: {error}",
            total = NUM_PODS,
            created = pods_created,
            ready = pods_ready,
            deleted = pods_deleted,
            error = error
        );
    }
}

fn partition_results<T, E>(results: Vec<Result<T, E>>) -> (Vec<T>, Vec<E>)
where
    E: fmt::Debug,
    T: fmt::Debug,
{
    let (successes, errors) = results.into_iter().partition::<Vec<_>, _>(Result::is_ok);
    let unwrapped_successes = successes.into_iter().map(Result::unwrap).collect();
    let unwrapped_errors = errors.into_iter().map(Result::unwrap_err).collect();

    (unwrapped_successes, unwrapped_errors)
}
