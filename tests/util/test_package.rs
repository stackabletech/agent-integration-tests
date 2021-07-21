use flate2::{write::GzEncoder, Compression};

/// Package with a shell script used for testing
#[derive(Clone, Debug)]
pub struct TestPackage {
    pub name: String,
    pub version: String,
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
    pub fn pod_spec(&self, pod_name: &str) -> String {
        format!(
            "
            apiVersion: v1
            kind: Pod
            metadata:
              name: {pod_name}
            spec:
              containers:
                - name: {package_name}
                  image: {package_name}:{package_version}
                  command:
                    - {command}
              nodeSelector:
                kubernetes.io/arch: stackable-linux
              tolerations:
                - key: kubernetes.io/arch
                  operator: Equal
                  value: stackable-linux
            ",
            pod_name = pod_name,
            package_name = self.name,
            package_version = self.version,
            command = self.command()
        )
    }
}
