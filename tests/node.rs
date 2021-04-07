mod test;
use test::prelude::*;

#[test]
fn at_least_one_node_should_be_available() {
    let contains_only_stackable_taints = |node: &Node| {
        node.spec
            .as_ref()
            .and_then(|spec| spec.taints.as_ref())
            .into_iter()
            .flatten()
            .all(|taint| {
                taint.key == "kubernetes.io/arch"
                    && taint.value == Some(String::from("stackable-linux"))
            })
    };

    let client = TestKubeClient::new();
    let mut nodes = client
        .list_labeled::<Node>("kubernetes.io/arch=stackable-linux")
        .items;
    nodes.retain(contains_only_stackable_taints);

    assert_that(&nodes).is_not_empty();
}

#[test]
fn nodes_should_be_tainted() {
    let client = TestKubeClient::new();
    let nodes = client.list_labeled::<Node>("kubernetes.io/arch=stackable-linux");

    for node in nodes {
        let taints = node
            .spec
            .and_then(|spec| spec.taints)
            .unwrap_or_else(Vec::new);

        assert_that(&taints).contains_all_of(&vec![
            &from_value(json!({
                "effect": "NoSchedule",
                "key": "kubernetes.io/arch",
                "value": "stackable-linux"
            })),
            &from_value(json!({
                "effect": "NoExecute",
                "key": "kubernetes.io/arch",
                "value": "stackable-linux"
            })),
        ]);
    }
}
