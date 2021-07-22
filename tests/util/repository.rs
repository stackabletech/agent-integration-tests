use anyhow::anyhow;
use anyhow::Result;
use http::Uri;
use integration_test_commons::test::prelude::*;
use nix::ifaddrs;
use nix::net::if_::InterfaceFlags;
use nix::sys::socket::SockAddr;
use serde::Serialize;
use sha2::{Digest, Sha512};
use std::net::IpAddr;
use std::{collections::HashMap, net::SocketAddr};
use tokio::sync::oneshot::{self, Sender};
use warp::{path::FullPath, Filter};

use super::test_package::TestPackage;

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

/// Named Stackable repository with test packages
pub struct StackableRepository {
    pub name: String,
    pub packages: Vec<TestPackage>,
}

/// A running instance of a Stackable repository
pub struct StackableRepositoryInstance {
    name: String,
    repository: Repository,
    shutdown_sender: Sender<()>,
}

impl StackableRepositoryInstance {
    /// Creates a new instance of a Stackable repository
    ///
    /// A web server is started providing the repository content and
    /// the repository is created on the Kubernetes API server.
    ///
    /// `close` must be called to stop and clean up this instance.
    pub async fn new(
        stackable_repository: &StackableRepository,
        client: &KubeClient,
    ) -> Result<Self> {
        match serve(&stackable_repository.packages) {
            Ok((address, shutdown_sender)) => {
                match register(client, &stackable_repository.name, &address).await {
                    Ok(repository) => {
                        let instance = StackableRepositoryInstance {
                            name: stackable_repository.name.to_owned(),
                            repository,
                            shutdown_sender,
                        };
                        Ok(instance)
                    }
                    Err(error) => {
                        let _ = shutdown_sender.send(());
                        Err(error)
                    }
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Closes the Stackable repository instance
    ///
    /// The repository is deleted on the Kubernetes API server and the
    /// web server is shut down.
    pub async fn close(self, client: &KubeClient) -> Result<()> {
        let mut errors = Vec::new();

        if let Err(error) = client.delete(self.repository).await {
            errors.push(format!(
                "Repository [{:?}] could not be deleted: {:?}",
                self.name, error
            ));
        }

        if let Err(()) = self.shutdown_sender.send(()) {
            errors.push(format!(
                "Repository server [{:?}] could not be shut down",
                self.name
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(errors.join("; ")))
        }
    }
}

/// Starts a web server providing a Stackable repository with the given
/// test packages.
///
/// The web server is bound to the IP address of the default interface
/// on an ephemeral port.
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

/// Returns the IP address of a network interface which is up and which
/// is not the loopback interface.
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
    address: &SocketAddr,
) -> Result<Repository> {
    let uri = Uri::builder()
        .scheme("http")
        .authority(address.to_string().as_str())
        .path_and_query("/")
        .build()
        .unwrap();

    let spec = formatdoc!(
        "
        apiVersion: stable.stackable.de/v1
        kind: Repository
        metadata:
            name: {}
            namespace: default
        spec:
            repo_type: StackableRepo
            properties:
                url: {}
        ",
        repository_name,
        uri
    );

    client.create::<Repository>(&spec).await
}
