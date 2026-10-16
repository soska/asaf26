//! IDs must match `@duckalization/id`. `id-vectors.json` was generated once
//! by running duckalization's own `messageId` and is frozen here.

use asaf_i18n::id::{canonical, message_id};
use asaf_i18n::Message;
use serde::Deserialize;

#[derive(Deserialize)]
struct Vector {
    message: Message,
    context: Option<String>,
    canonical: String,
    id: String,
}

#[test]
fn ids_match_duckalization() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!("id-vectors.json")).unwrap();
    assert!(vectors.len() > 10);
    for v in vectors {
        assert_eq!(canonical(&v.message), v.canonical, "canonical form of {:?}", v.message);
        assert_eq!(message_id(&v.message, v.context.as_deref()), v.id, "id of {:?} ({:?})", v.message, v.context);
    }
}
