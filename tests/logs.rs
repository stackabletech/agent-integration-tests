mod test;
use test::prelude::*;

struct EchoService<'a> {
    client: &'a TestKubeClient,
    pod: TemporaryResource<'a, Pod>,
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
            &formatdoc! {r#"
                apiVersion: v1
                kind: Pod
                metadata:
                  name: agent-logs-integration-test-{id}
                spec:
                  containers:
                    - name: echo-service
                      image: echo-service:1.0.0
                      command:
                        - echo-service-1.0.0/start.sh
                      env:
                        - name: LOG_OUTPUT
                          value: "{log_output}"
                  tolerations:
                    - key: kubernetes.io/arch
                      operator: Equal
                      value: stackable-linux
                "#,
                id = Uuid::new_v4(),
                log_output = log_output.join(NEWLINE)
            },
        );

        client.verify_pod_condition(&pod, "Ready");

        EchoService { client, pod }
    }

    pub fn get_logs(&self, tail_lines: Option<i64>) -> Vec<String> {
        self.client.get_logs(&self.pod, tail_lines)
    }
}

#[test]
fn all_logs_should_be_retrievable() {
    let client = TestKubeClient::new();

    let log_output = vec!["line 1", "line 2", "line 3"];
    let echo_service = EchoService::new(&client, &log_output);

    let logs = echo_service.get_logs(None);
    assert_equals(&["line 1", "line 2", "line 3"], &logs);
}

#[test]
fn the_tail_of_logs_should_be_retrievable() {
    let client = TestKubeClient::new();

    let log_output = vec!["line 1", "line 2", "line 3"];
    let echo_service = EchoService::new(&client, &log_output);

    let logs = echo_service.get_logs(Some(0));
    assert_that(&logs).is_empty();

    let logs = echo_service.get_logs(Some(1));
    assert_equals(&["line 3"], &logs);

    let logs = echo_service.get_logs(Some(2));
    assert_equals(&["line 2", "line 3"], &logs);

    let logs = echo_service.get_logs(Some(3));
    assert_equals(&["line 1", "line 2", "line 3"], &logs);

    let logs = echo_service.get_logs(Some(4));
    assert_equals(&["line 1", "line 2", "line 3"], &logs);
}

#[test]
fn non_ascii_characters_should_be_handled_correctly() {
    let client = TestKubeClient::new();

    let log_output = vec!["Spade: ♠", "Heart: ♥", "Diamond: ♦", "Club: ♣"];
    let echo_service = EchoService::new(&client, &log_output);

    let logs = echo_service.get_logs(None);
    assert_equals(&["Spade: ♠", "Heart: ♥", "Diamond: ♦", "Club: ♣"], &logs);
}

fn assert_equals(expected: &[&str], actual: &[String]) {
    assert_that(&actual.iter().map(String::as_ref).collect::<Vec<_>>())
        .equals_iterator(&expected.iter());
}
