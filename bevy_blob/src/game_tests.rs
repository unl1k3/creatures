#[cfg(test)]
mod tests {
    use super::*;
    #[path = "game_tests/rejoin.rs"]
    mod rejoin;

    #[path = "game_tests/contacts.rs"]
    mod contacts;

    #[path = "game_tests/presentation.rs"]
    mod presentation;

    #[path = "game_tests/scenarios.rs"]
    mod scenarios;

    fn active(id: u64, parent_id: Option<u64>, body: Blob) -> ActiveBlob {
        ActiveBlob {
            id,
            parent_id,
            body,
        }
    }

    fn sibling_world(first: Blob, second: Blob, rejoining: bool) -> BlobWorld {
        BlobWorld {
            active: vec![active(1, Some(0), first), active(2, Some(0), second)],
            selected: 0,
            rejoin_parent: rejoining.then_some(0),
            rejoin_elapsed: 0.0,
            parent_links: HashMap::from([(0, None)]),
            next_id: 3,
        }
    }

}
