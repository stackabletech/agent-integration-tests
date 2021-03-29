//! Clients for interacting with the Kubernetes API
//!
//! These clients simplify testing.

use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::api::{
    Api, DeleteParams, ListParams, Meta, ObjectList, Patch, PatchParams, PostParams, WatchEvent,
};
use kube::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::runtime::Runtime;

/// A client for interacting with the Kubernetes API
///
/// [`TestKubeClient`] is a synchronous version of [`KubeClient`] which
/// additionally panics on erroneous results. It reduces the verbosity of
/// test cases.
pub struct TestKubeClient {
    runtime: Runtime,
    kube_client: KubeClient,
}

impl TestKubeClient {
    /// Creates a [`TestKubeClient`].
    pub fn new() -> TestKubeClient {
        let runtime = Runtime::new().expect("Tokio runtime could not be created");
        let kube_client = runtime.block_on(async {
            KubeClient::new()
                .await
                .expect("Kubernetes client could not be created")
        });
        TestKubeClient {
            runtime,
            kube_client,
        }
    }

    /// Gets a list of resources restricted by the label selector.
    ///
    /// The label selector supports `=`, `==`, `!=`, and can be comma
    /// separated: `key1=value1,key2=value2`.
    pub fn list_labeled<K>(&self, label_selector: &str) -> ObjectList<K>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta,
    {
        self.runtime.block_on(async {
            self.kube_client
                .list_labeled(label_selector)
                .await
                .expect("List of Stackable nodes could not be retrieved")
        })
    }

    /// Applies the given custom resource definition and blocks until it is accepted.
    pub fn apply_crd(&self, crd: &CustomResourceDefinition) {
        self.runtime.block_on(async {
            self.kube_client
                .apply_crd(crd)
                .await
                .expect("Custom resource definition coult not be applied")
        })
    }

    /// Searches for a named resource.
    pub fn find<K>(&self, name: &str) -> Option<K>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta,
    {
        self.runtime
            .block_on(async { self.kube_client.find::<K>(name).await })
    }

    /// Applies a resource with the given YAML specification.
    pub fn apply<K>(&self, spec: &str) -> K
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta + Serialize,
    {
        self.runtime.block_on(async {
            self.kube_client
                .apply::<K>(spec)
                .await
                .expect("Resource could not be applied")
        })
    }

    /// Creates a resource with the given YAML specification.
    pub fn create<K>(&self, spec: &str) -> K
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta + Serialize,
    {
        self.runtime.block_on(async {
            self.kube_client
                .create(spec)
                .await
                .expect("Resource could not be created")
        })
    }

    /// Deletes the given resource.
    pub fn delete<K>(&self, resource: K)
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta,
    {
        self.runtime.block_on(async {
            self.kube_client
                .delete(resource)
                .await
                .expect("Resource could not be deleted")
        })
    }

    /// Verifies that the given pod condition becomes true within 30 seconds.
    pub fn verify_pod_condition(&self, pod: &Pod, condition_type: &str) {
        self.runtime.block_on(async {
            self.kube_client
                .verify_pod_condition(pod, condition_type)
                .await
                .expect("Pod condition could not be verified")
        })
    }
}

/// A client for interacting with the Kubernetes API
///
/// [`KubeClient`] wraps a [`Client`][kube::Client]. It provides methods
/// which are less verbose and await the according status change within
/// defined timeouts.
pub struct KubeClient {
    client: Client,
    namespace: String,
}

impl KubeClient {
    /// Creates a [`KubeClient`].
    pub async fn new() -> Result<KubeClient> {
        let client = Client::try_default().await?;
        Ok(KubeClient {
            client,
            namespace: String::from("default"),
        })
    }

    /// Gets a list of resources restricted by the label selector.
    ///
    /// The label selector supports `=`, `==`, `!=`, and can be comma separated:
    /// `key1=value1,key2=value2`.
    pub async fn list_labeled<K>(&self, label_selector: &str) -> Result<ObjectList<K>>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta,
    {
        let api: Api<K> = Api::all(self.client.clone());
        let lp = ListParams::default().labels(label_selector);
        Ok(api.list(&lp).await?)
    }

    /// Applies the given custom resource definition and awaits the accepted status.
    pub async fn apply_crd(&self, crd: &CustomResourceDefinition) -> anyhow::Result<()> {
        let is_ready = |crd: &CustomResourceDefinition| {
            crd.status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .and_then(|conditions| conditions.iter().find(|c| c.type_ == "NamesAccepted"))
                .map(|condition| &condition.status)
                .filter(|status| *status == "True")
                .is_some()
        };

        let timeout_secs = 30;
        let crds: Api<CustomResourceDefinition> = Api::all(self.client.clone());

        let apply_params = PatchParams::apply("agent_integration_test").force();
        crds.patch(&crd.name(), &apply_params, &Patch::Apply(crd))
            .await?;

        if crds.get(&crd.name()).await.is_ok() {
            return Ok(());
        }

        let lp = ListParams::default()
            .fields(&format!("metadata.name={}", crd.name()))
            .timeout(timeout_secs);
        let mut stream = crds.watch(&lp, "0").await?.boxed();

        while let Some(status) = stream.try_next().await? {
            println!("{:?}", status);
            if let WatchEvent::Modified(crd) = status {
                if is_ready(&crd) {
                    return Ok(());
                }
            }
        }

        Err(anyhow::anyhow!(
            "Custom resource definition [{}] could not be applied within {} seconds.",
            crd.name(),
            timeout_secs
        ))
    }

    /// Searches for a named resource.
    pub async fn find<K>(&self, name: &str) -> Option<K>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta,
    {
        let api: Api<K> = Api::namespaced(self.client.clone(), &self.namespace);
        api.get(name).await.ok()
    }

    /// Applies a resource with the given YAML specification.
    pub async fn apply<K>(&self, spec: &str) -> Result<K>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta + Serialize,
    {
        let resource: K = from_yaml(spec);
        let apply_params = PatchParams::apply("agent_integration_test").force();
        let api: Api<K> = Api::namespaced(self.client.clone(), &self.namespace);
        Ok(api
            .patch(&resource.name(), &apply_params, &Patch::Apply(&resource))
            .await?)
    }

    /// Creates a resource with the given YAML specification and awaits the
    /// confirmation of the creation.
    pub async fn create<K>(&self, spec: &str) -> Result<K>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta + Serialize,
    {
        let timeout_secs = 10;
        let api: Api<K> = Api::namespaced(self.client.clone(), &self.namespace);

        let resource = from_yaml(spec);
        api.create(&PostParams::default(), &resource).await?;

        let lp = ListParams::default()
            .fields(&format!("metadata.name={}", resource.name()))
            .timeout(timeout_secs);
        let mut stream = api.watch(&lp, "0").await?.boxed();

        while let Some(status) = stream.try_next().await? {
            if let WatchEvent::Added(resource) = status {
                return Ok(resource);
            }
        }

        Err(anyhow::anyhow!(
            "Resource [{}] could not be created within {} seconds.",
            resource.name(),
            timeout_secs
        ))
    }

    /// Deletes the given resource and awaits the confirmation of the deletion.
    pub async fn delete<K>(&self, resource: K) -> Result<()>
    where
        K: k8s_openapi::Resource + Clone + DeserializeOwned + Meta,
    {
        let timeout_secs = 10;
        let api: Api<K> = Api::namespaced(self.client.clone(), &self.namespace);

        let result = api
            .delete(&resource.name(), &DeleteParams::default())
            .await?;

        if result.is_right() {
            return Ok(());
        }

        let lp = ListParams::default()
            .fields(&format!("metadata.name={}", resource.name()))
            .timeout(timeout_secs);
        let mut stream = api.watch(&lp, "0").await?.boxed();

        while let Some(status) = stream.try_next().await? {
            if let WatchEvent::Deleted(_) = status {
                return Ok(());
            }
        }

        Err(anyhow::anyhow!(
            "Resource [{}] could not be deleted within {} seconds.",
            resource.name(),
            timeout_secs
        ))
    }

    /// Verifies that the given pod condition becomes true within 30 seconds.
    pub async fn verify_pod_condition(
        &self,
        pod: &Pod,
        condition_type: &str,
    ) -> anyhow::Result<()> {
        let is_condition_true = |pod: &Pod| {
            pod.status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .and_then(|conditions| conditions.iter().find(|c| c.type_ == condition_type))
                .map(|condition| &condition.status)
                .filter(|status| *status == "True")
                .is_some()
        };

        if is_condition_true(&pod) {
            return Ok(());
        }

        let timeout_secs = 30;
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let lp = ListParams::default()
            .fields(&format!("metadata.name={}", pod.name()))
            .timeout(timeout_secs);
        let mut stream = pods.watch(&lp, "0").await?.boxed();

        while let Some(status) = stream.try_next().await? {
            if let WatchEvent::Modified(pod) = status {
                if is_condition_true(&pod) {
                    return Ok(());
                }
            }
        }

        Err(anyhow::anyhow!(
            "Pod condition [{}] was not satisfied within {} seconds",
            condition_type,
            timeout_secs
        ))
    }
}

/// Deserializes the given JSON value into the desired type.
pub fn from_value<T>(value: Value) -> T
where
    T: DeserializeOwned,
{
    T::deserialize(value).expect("Deserialization failed")
}

/// Deserializes the given YAML text into the desired type.
pub fn from_yaml<T>(str: &str) -> T
where
    T: DeserializeOwned,
{
    serde_yaml::from_str(str).expect("String is not a well-formed YAML")
}
