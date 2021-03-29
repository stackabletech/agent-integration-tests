mod test;
use test::prelude::*;

#[test]
fn at_least_one_node_should_be_available() {
    let client = TestKubeClient::new();
    let nodes = client.list_labeled::<Node>("kubernetes.io/arch=stackable-linux");

    assert_that(&nodes.items).is_not_empty();
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

        assert_that(&taints).contains_exactly_in_any_order(&vec![
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
