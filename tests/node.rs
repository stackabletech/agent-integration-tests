use integration_test_commons::test::prelude::*;

#[test]
fn at_least_one_node_should_be_available() {
    let client = TestKubeClient::new();

    let mut nodes = client
        .list_labeled::<Node>("kubernetes.io/arch=stackable-linux")
        .items;

    let contains_only_stackable_taints = |node: &Node| {
        get_node_taints(node).iter().all(|taint| {
            taint.key == "kubernetes.io/arch"
                && taint.value == Some(String::from("stackable-linux"))
        })
    };
    nodes.retain(contains_only_stackable_taints);

    let is_ready = |node: &Node| {
        get_node_conditions(node)
            .iter()
            .any(|condition| condition.type_ == "Ready" && condition.status == "True")
    };
    nodes.retain(is_ready);

    assert_that(&nodes).is_not_empty();
}

#[test]
fn nodes_should_be_tainted() {
    let client = TestKubeClient::new();
    let nodes = client.list_labeled::<Node>("kubernetes.io/arch=stackable-linux");

    for node in nodes {
        let taints = get_node_taints(&node);

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
