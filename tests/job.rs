use integration_test_commons::test::prelude::*;

struct ExitService<'a> {
    client: &'a TestKubeClient,
    pod: TemporaryResource<'a, Pod>,
}

impl<'a> ExitService<'a> {
    pub fn new(client: &'a TestKubeClient, exit_code: i32) -> Self {
        setup_repository(&client);

        let pod = TemporaryResource::new(
            &client,
            &with_unique_name(&formatdoc!(
                "
                apiVersion: v1
                kind: Pod
                metadata:
                  name: agent-service-integration-test-job
                spec:
                  containers:
                    - name: exit-service
                      image: exit-service:1.0.0
                      command:
                        - exit-service-1.0.0/start.sh
                      env:
                        - name: EXIT_CODE
                          value: {exit_code}
                  restartPolicy: Never
                  nodeSelector:
                    kubernetes.io/arch: stackable-linux
                  tolerations:
                    - key: kubernetes.io/arch
                      operator: Equal
                      value: stackable-linux
                ",
                exit_code = exit_code
            )),
        );

        ExitService { client, pod }
    }

    pub fn verify_terminated(&mut self) {
        self.client.verify_status::<Pod, _>(&self.pod, |pod| {
            let phase = ExitService::phase_from(pod);
            let container_terminated = ExitService::terminated_container_state_from(pod).is_some();
            (phase == "Succeeded" || phase == "Failed") && container_terminated
        });

        self.pod.update();
    }

    pub fn phase(&self) -> String {
        ExitService::phase_from(&self.pod)
    }

    pub fn terminated_container_state(&self) -> Option<ContainerStateTerminated> {
        ExitService::terminated_container_state_from(&self.pod)
    }

    fn phase_from(pod: &Pod) -> String {
        pod.status
            .as_ref()
            .and_then(|status| status.phase.clone())
            .unwrap_or_else(|| String::from("Unknown"))
    }

    fn terminated_container_state_from(pod: &Pod) -> Option<ContainerStateTerminated> {
        pod.status
            .as_ref()
            .and_then(|status| status.container_statuses.first())
            .and_then(|container_status| container_status.state.as_ref())
            .and_then(|state| state.terminated.to_owned())
    }
}

#[test]
fn successful_job_should_have_phase_succeeded_and_error_code_0() {
    let client = TestKubeClient::new();

    let exit_code = 0;
    let mut exit_service = ExitService::new(&client, exit_code);

    exit_service.verify_terminated();

    asserting("phase")
        .that(&exit_service.phase())
        .is_equal_to(String::from("Succeeded"));

    let container_state = exit_service
        .terminated_container_state()
        .expect("Terminated container state expected");
    asserting("exit code")
        .that(&container_state.exit_code)
        .is_equal_to(0);
    asserting("message")
        .that(&container_state.message)
        .is_equal_to(Some(String::from("Completed")));
    asserting("reason")
        .that(&container_state.message)
        .is_equal_to(Some(String::from("Completed")));
}

#[test]
fn failed_job_should_have_phase_failed_and_error_code_1() {
    let client = TestKubeClient::new();

    // All non-zero exit codes are mapped by the agent to 1.
    let exit_code = 42;
    let mut exit_service = ExitService::new(&client, exit_code);

    exit_service.verify_terminated();

    asserting("phase")
        .that(&exit_service.phase())
        .is_equal_to(String::from("Failed"));

    let container_state = exit_service
        .terminated_container_state()
        .expect("Terminated container state expected");
    asserting("exit code")
        .that(&container_state.exit_code)
        .is_equal_to(1);
    asserting("message")
        .that(&container_state.message)
        .is_equal_to(Some(String::from("Error")));
    asserting("reason")
        .that(&container_state.message)
        .is_equal_to(Some(String::from("Error")));
}
