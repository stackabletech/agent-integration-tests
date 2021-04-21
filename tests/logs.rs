mod test;
use test::prelude::*;

/// Newline character for LOG_OUTPUT
///
/// Source code: \\\\\\\\n
/// Pod spec: \\\\n
/// Systemd unit file: \\n
/// Standard output: \n
/// Journal: separate entries
const NEWLINE: &str = "\\\\\\\\n";

struct EchoService<'a> {
    client: &'a TestKubeClient,
    pod: Option<Pod>,
}

impl<'a> EchoService<'a> {
    pub fn new(client: &'a TestKubeClient, log_output: &[&str]) -> Self {
        let pod = client.create(&formatdoc! {r#"
            apiVersion: v1
            kind: Pod
            metadata:
              name: agent-logs-integration-test
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
            log_output = log_output.join(NEWLINE)
        });

        client.verify_pod_condition(&pod, "Ready");

        EchoService {
            client,
            pod: Some(pod),
        }
    }

    pub fn get_logs(&self, tail_lines: Option<i64>) -> Vec<String> {
        self.client.get_logs(self.pod.as_ref().unwrap(), tail_lines)
    }
}

impl<'a> Drop for EchoService<'a> {
    fn drop(&mut self) {
        let pod = self.pod.take().unwrap();
        self.client.delete(pod);
    }
}

#[test]
fn logs_should_be_accessible() {
    let client = TestKubeClient::new();

    create_repository(&client);

    let log_output = vec!["line 1", "line 2", "line 3"];
    let echo_service = EchoService::new(&client, &log_output);

    let logs = echo_service.get_logs(None);
    assert_equals(&["line 1", "line 2"], &logs);

    let logs = echo_service.get_logs(Some(0));
    assert_that(&logs).is_empty();

    let logs = echo_service.get_logs(Some(1));
    assert_equals(&["line 2"], &logs);

    let logs = echo_service.get_logs(Some(2));
    assert_equals(&["line 1", "line 2"], &logs);

    let logs = echo_service.get_logs(Some(3));
    assert_equals(&["line 1", "line 2"], &logs);
}

fn assert_equals(expected: &[&str], actual: &[String]) {
    assert_that(&actual.iter().map(String::as_ref).collect::<Vec<_>>())
        .equals_iterator(&expected.iter());
}

fn create_repository(client: &TestKubeClient) {
    client.apply_crd(&Repository::crd());

    client.apply::<Repository>(indoc! {"
        apiVersion: stable.stackable.de/v1
        kind: Repository
        metadata:
            name: integration-test-repository
            namespace: default
        spec:
            repo_type: StackableRepo
            properties:
                url: http://localhost:8082/
    "});
}
