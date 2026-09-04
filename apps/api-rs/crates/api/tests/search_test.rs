use api::routes::search::{parse_entities, SEARCH_ENTITIES};

#[test]
fn filters_unknown_entities() {
    let entities = parse_entities(Some("issue,page,bogus,,cycle"));
    assert_eq!(entities, vec!["issue", "page", "cycle"]);
}

#[test]
fn defaults_to_all_entities() {
    for param in [None, Some(""), Some("   ")] {
        let entities = parse_entities(param);
        assert_eq!(entities.len(), SEARCH_ENTITIES.len());
    }
}
