use integration_test_commons::test::prelude::*;

struct EchoService<'a> {
    client: &'a TestKubeClient,
    pod: TemporaryResource<'a, Pod>,
    pub logs_enabled: bool,
}

impl<'a> EchoService<'a> {
    pub fn new(client: &'a TestKubeClient, log_output: &[&str]) -> Self {
        setup_repository(&client);

        /// Newline character for LOG_OUTPUT
        ///
        /// Source code:        \\\\\\\\n
        /// Pod spec:           \\\\n
        /// Systemd unit file:  \\n
        /// echo-service:       \n
        /// Journal:            separate entries
        const NEWLINE: &str = "\\\\\\\\n";

        let pod = TemporaryResource::new(
            &client,
            &with_unique_name(&formatdoc!(
                r#"
                apiVersion: v1
                kind: Pod
                metadata:
                  name: agent-logs-integration-test-logs
                spec:
                  containers:
                    - name: echo-service
                      image: echo-service:1.0.0
                      command:
                        - echo-service-1.0.0/start.sh
                      env:
                        - name: LOG_OUTPUT
                          value: "{log_output}"
                  nodeSelector:
                    kubernetes.io/arch: stackable-linux
                  tolerations:
                    - key: kubernetes.io/arch
                      operator: Equal
                      value: stackable-linux
                "#,
                log_output = log_output.join(NEWLINE)
            )),
        );

        client.verify_pod_condition(&pod, "Ready");

        const ANNOTATION_KEY_FEATURE_LOGS: &str = "featureLogs";

        let logs_enabled = match client
            .get_annotation::<Pod>(&pod, ANNOTATION_KEY_FEATURE_LOGS)
            .as_ref()
        {
            "true" => true,
            "false" => false,
            value => panic!(
                "Pod annotation [{}] contains unknown value [{}]; \
                expected [true] or [false]",
                ANNOTATION_KEY_FEATURE_LOGS, value,
            ),
        };

        EchoService {
            client,
            pod,
            logs_enabled,
        }
    }

    pub fn get_logs(&self, params: &LogParams) -> Vec<String> {
        self.client.get_logs(&self.pod, params)
    }
}

#[test]
fn all_logs_should_be_retrievable() {
    let client = TestKubeClient::new();

    let log_output = vec!["line 1", "line 2", "line 3"];
    let echo_service = EchoService::new(&client, &log_output);

    let logs = echo_service.get_logs(&LogParams::default());

    if echo_service.logs_enabled {
        assert_equals(&["line 1", "line 2", "line 3"], &logs);
    } else {
        assert_that(&logs).is_empty();
    }
}

#[test]
fn the_tail_of_logs_should_be_retrievable() {
    let client = TestKubeClient::new();

    let log_output = vec!["line 1", "line 2", "line 3"];
    let echo_service = EchoService::new(&client, &log_output);

    let with_tail_lines = |tail_lines| LogParams {
        tail_lines: Some(tail_lines),
        ..Default::default()
    };

    let logs = echo_service.get_logs(&with_tail_lines(0));
    assert_that(&logs).is_empty();

    if echo_service.logs_enabled {
        let logs = echo_service.get_logs(&with_tail_lines(1));
        assert_equals(&["line 3"], &logs);

        let logs = echo_service.get_logs(&with_tail_lines(2));
        assert_equals(&["line 2", "line 3"], &logs);

        let logs = echo_service.get_logs(&with_tail_lines(3));
        assert_equals(&["line 1", "line 2", "line 3"], &logs);

        let logs = echo_service.get_logs(&with_tail_lines(4));
        assert_equals(&["line 1", "line 2", "line 3"], &logs);
    }
}

#[test]
fn non_ascii_characters_should_be_handled_correctly_in_the_logs() {
    let client = TestKubeClient::new();

    let log_output = vec!["Spade: ♠", "Heart: ♥", "Diamond: ♦", "Club: ♣"];
    let echo_service = EchoService::new(&client, &log_output);

    let logs = echo_service.get_logs(&LogParams::default());

    if echo_service.logs_enabled {
        assert_equals(&["Spade: ♠", "Heart: ♥", "Diamond: ♦", "Club: ♣"], &logs);
    } else {
        assert_that(&logs).is_empty();
    }
}

fn assert_equals(expected: &[&str], actual: &[String]) {
    assert_that(&actual.iter().map(String::as_ref).collect::<Vec<_>>())
        .equals_iterator(&expected.iter());
}
