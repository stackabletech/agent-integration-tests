use std::collections::BTreeMap;

use flate2::{write::GzEncoder, Compression};
use integration_test_commons::test::prelude::{Container, Pod, PodSpec, Toleration};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/// Package with a shell script used for testing
#[derive(Clone, Debug)]
pub struct TestPackage {
    pub name: String,
    pub version: String,
    pub job: bool,
    pub script: String,
}

impl TestPackage {
    /// Returns the packaged script as .tar.gz
    pub fn binary(&self) -> Vec<u8> {
        let enc = GzEncoder::new(Vec::new(), Compression::default());
        let mut tar = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(self.script.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        tar.append_data(&mut header, self.command(), self.script.as_bytes())
            .unwrap();

        tar.into_inner().unwrap().finish().unwrap()
    }

    /// Returns the filename of the packaged script
    pub fn filename(&self) -> String {
        format!("{}-{}.tar.gz", self.name, self.version)
    }

    /// Returns the repository path where the package should be provided
    pub fn repository_path(&self) -> String {
        format!("{}/{}", self.name, self.filename())
    }

    /// Returns the command which is used to start the script after
    /// unpacking
    pub fn command(&self) -> String {
        format!("{}-{}/start.sh", self.name, self.version)
    }

    /// Creates a pod specification for this package
    pub fn pod(&self, pod_name: &str) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(String::from(pod_name)),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: self.name.to_owned(),
                    image: Some(format!("{}:{}", self.name, self.version)),
                    command: vec![self.command()],
                    ..Default::default()
                }],
                node_selector: {
                    let mut selectors = BTreeMap::new();
                    selectors.insert(
                        String::from("kubernetes.io/arch"),
                        String::from("stackable-linux"),
                    );
                    selectors
                },
                restart_policy: Some(String::from(if self.job { "Never" } else { "Always" })),
                tolerations: vec![Toleration {
                    key: Some(String::from("kubernetes.io/arch")),
                    operator: Some(String::from("Equal")),
                    value: Some(String::from("stackable-linux")),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
