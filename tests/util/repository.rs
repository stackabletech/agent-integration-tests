use std::net::IpAddr;
use std::{collections::HashMap, net::SocketAddr};

use anyhow::anyhow;
use anyhow::Result;
use http::Uri;
use integration_test_commons::test::kube::KubeClient;
use kube::CustomResource;
use nix::ifaddrs;
use nix::net::if_::InterfaceFlags;
use nix::sys::socket::SockAddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use tokio::sync::oneshot::{self, Sender};
use warp::{path::FullPath, Filter};

use super::test_package::TestPackage;

/// Specification of a Stackable repository
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "Repository",
    group = "stable.stackable.de",
    version = "v1",
    namespaced
)]
pub struct RepositorySpec {
    pub repo_type: String,
    pub properties: HashMap<String, String>,
}

/// A specific version of a package in a Stackable repository
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PackageVersion {
    version: String,
    path: String,
    hashes: HashMap<String, String>,
}

impl From<&TestPackage> for PackageVersion {
    fn from(package: &TestPackage) -> Self {
        let binary = package.binary();

        let hash = format!("{:x}", Sha512::digest(&binary));

        let mut hashes = HashMap::new();
        hashes.insert(String::from("SHA512"), hash);

        PackageVersion {
            version: package.version.to_owned(),
            path: package.repository_path(),
            hashes,
        }
    }
}

/// Content of a metadata file in a Stackable repository
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct StackableRepositoryMetadata {
    version: String,
    packages: HashMap<String, Vec<PackageVersion>>,
}

impl From<&[TestPackage]> for StackableRepositoryMetadata {
    fn from(test_packages: &[TestPackage]) -> Self {
        let mut packages = HashMap::new();

        for package in test_packages {
            let mut package_versions: Vec<_> = packages.remove(&package.name).unwrap_or_default();
            package_versions.push(package.into());
            packages.insert(package.name.to_owned(), package_versions);
        }

        StackableRepositoryMetadata {
            version: String::from("1"),
            packages,
        }
    }
}

/// Builder for a Stackable repository with test packages
pub struct StackableRepositoryBuilder {
    name: String,
    repo_type: String,
    packages: Vec<TestPackage>,
    serve: bool,
    uri: Option<String>,
}

impl StackableRepositoryBuilder {
    /// Creates an instance with the given repository name and type `StackableRepo`.
    pub fn new(name: &str) -> Self {
        StackableRepositoryBuilder {
            name: String::from(name),
            repo_type: String::from("StackableRepo"),
            packages: Vec::new(),
            serve: true,
            uri: None,
        }
    }

    /// Changes the repo type.
    #[allow(dead_code)]
    pub fn repo_type(&mut self, repo_type: &str) -> &Self {
        self.repo_type = repo_type.to_owned();
        self
    }

    /// Adds the given package to the repository.
    #[allow(dead_code)]
    pub fn package(&mut self, package: &TestPackage) -> &Self {
        self.packages.push(package.to_owned());
        self
    }

    /// Sets an URI.
    ///
    /// A web server serving the given packages will not be started.
    #[allow(dead_code)]
    pub fn uri(&mut self, uri: &Option<String>) -> &Self {
        self.serve = false;
        self.uri = uri.to_owned();
        self
    }

    /// Creates a new instance of a Stackable repository
    ///
    /// If `uri` was not changed then a web server is started providing the repository content. The
    /// repository is created on the Kubernetes API server.
    ///
    /// [`StackableRepositoryInstance::close`] must be called to stop and clean up this instance.
    pub async fn run(&self, client: &KubeClient) -> Result<StackableRepositoryInstance> {
        let (uri, shutdown_sender) = if self.serve {
            let (address, shutdown_sender) = serve(&self.packages)?;
            let uri = Uri::builder()
                .scheme("http")
                .authority(address.to_string().as_str())
                .path_and_query("/")
                .build()
                .unwrap()
                .to_string();
            (Some(uri), Some(shutdown_sender))
        } else {
            (self.uri.to_owned(), None)
        };

        match register(client, &self.name, &self.repo_type, &uri).await {
            Ok(repository) => {
                let instance = StackableRepositoryInstance {
                    name: self.name.to_owned(),
                    repository,
                    shutdown_sender,
                };
                Ok(instance)
            }
            Err(error) => {
                if let Some(shutdown_sender) = shutdown_sender {
                    let _ = shutdown_sender.send(());
                };
                Err(error)
            }
        }
    }
}

/// A running instance of a Stackable repository
pub struct StackableRepositoryInstance {
    name: String,
    repository: Repository,
    shutdown_sender: Option<Sender<()>>,
}

impl StackableRepositoryInstance {
    /// Closes the Stackable repository instance
    ///
    /// The repository is deleted on the Kubernetes API server and the web server is shut down.
    pub async fn close(self, client: &KubeClient) -> Result<()> {
        let mut errors = Vec::new();

        if let Err(error) = client.delete(self.repository).await {
            errors.push(format!(
                "Repository [{:?}] could not be deleted: {:?}",
                self.name, error
            ));
        }

        if let Some(shutdown_sender) = self.shutdown_sender {
            if let Err(()) = shutdown_sender.send(()) {
                errors.push(format!(
                    "Repository server [{:?}] could not be shut down",
                    self.name
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(errors.join("; ")))
        }
    }
}

/// Starts a web server providing a Stackable repository with the given test packages.
///
/// The web server is bound to the IP address of the default interface on an ephemeral port.
fn serve(packages: &[TestPackage]) -> Result<(SocketAddr, Sender<()>)> {
    let ip_address = default_ip_address()?;
    let socket_address = SocketAddr::new(ip_address, 0);

    let packages_cloned = packages.to_owned();
    let metadata_route = warp::path("metadata.json").map(move || {
        warp::reply::json(&StackableRepositoryMetadata::from(packages_cloned.as_ref()))
    });

    let packages_cloned = packages.to_owned();
    let package_route = warp::path::full().and_then(move |path: FullPath| {
        let packages = packages_cloned.to_owned();

        async move {
            packages
                .iter()
                .find(|package| format!("/{}", package.repository_path()) == path.as_str())
                .map(|package| package.binary())
                .ok_or_else(warp::reject::not_found)
        }
    });

    let routes = metadata_route.or(package_route);

    let (tx, rx) = oneshot::channel::<()>();

    let (address, server) =
        warp::serve(routes).try_bind_with_graceful_shutdown(socket_address, async {
            rx.await.ok();
        })?;

    tokio::task::spawn(server);

    Ok((address, tx))
}

/// Returns the IP address of a network interface which is up and which is not the loopback
/// interface.
///
/// Usually the default IP address is returned.
fn default_ip_address() -> Result<IpAddr> {
    ifaddrs::getifaddrs()?
        .filter(|ifaddr| {
            ifaddr.flags.contains(InterfaceFlags::IFF_UP)
                && !ifaddr.flags.contains(InterfaceFlags::IFF_LOOPBACK)
        })
        .find_map(|ifaddr| {
            if let Some(SockAddr::Inet(inet_addr)) = ifaddr.address {
                Some(inet_addr.to_std().ip())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            anyhow!(
                "No network interface found which is up, bound to an \
                    IP address, and not the loopback interface"
            )
        })
}

/// Registers a Stackable repository on the Kubernetes API server
async fn register(
    client: &KubeClient,
    repository_name: &str,
    repository_type: &str,
    uri: &Option<String>,
) -> Result<Repository> {
    let repository = Repository::new(
        repository_name,
        RepositorySpec {
            repo_type: repository_type.to_owned(),
            properties: {
                let mut props = HashMap::new();

                if let Some(uri) = uri {
                    props.insert(String::from("url"), uri.to_owned());
                }

                props
            },
        },
    );

    client
        .create::<Repository>(&serde_yaml::to_string(&repository).unwrap())
        .await
}
