//! Tests of the Gest model port in `gossil::gest_model`.
//!
//! The modules `author`, `primitives`, `project` and `tag` hold the inherited
//! Gest model tests at the pin, one module per upstream file and test module.
//! Their bodies are the upstream bodies. Only the imports change, and the
//! project test reads the clock with `utc_now` because `chrono` has no clock
//! feature here. Four inherited `id.rs` tests need private items, so they stay
//! in `src/gest_model/primitives/id.rs`. The source map
//! `tests/fixtures/gest_model/source-map.json` names the port of each test.
//!
//! The module `port` holds new tests of what the port adds or changes. They
//! check the serde wire format, fallible identifier decoding, and the conversion
//! from each validated converter record into its model.

#[path = "support/gest_source_map.rs"]
#[allow(
    dead_code,
    reason = "this file uses only the pin, the addendum and the hash table of the shared support"
)]
mod gest_source_map;

use chrono::{DateTime, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

/// Read the system clock as `chrono::Utc::now` does, for the inherited clock test.
fn utc_now() -> DateTime<Utc> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    DateTime::from_timestamp(since.as_secs() as i64, since.subsec_nanos()).unwrap()
}

mod author {
    use gossil::gest_model::{author::Model, primitives::AuthorType};

    mod new {
        use super::*;

        #[test]
        fn it_creates_a_human_author() {
            let author = Model::new("Alice", AuthorType::Human);

            assert_eq!(author.name(), "Alice");
            assert_eq!(author.author_type(), AuthorType::Human);
            assert_eq!(author.email(), &None);
        }

        #[test]
        fn it_creates_an_agent_author() {
            let author = Model::new("Claude", AuthorType::Agent);

            assert_eq!(author.author_type(), AuthorType::Agent);
        }
    }

    mod with_email {
        use super::*;

        #[test]
        fn it_sets_the_email() {
            let author = Model::new("Alice", AuthorType::Human).with_email("alice@example.com");

            assert_eq!(author.email().as_deref(), Some("alice@example.com"));
        }
    }
}

mod project {
    use super::utc_now;
    use gossil::gest_model::project::Model;
    use std::path::PathBuf;

    mod new {
        use super::*;

        #[test]
        fn it_sets_timestamps_to_now() {
            let before = utc_now();
            let project = Model::new(PathBuf::from("/tmp/test"));
            let after = utc_now();

            assert!(*project.created_at() >= before && *project.created_at() <= after);
            assert_eq!(project.created_at(), project.updated_at());
        }
    }
}

mod tag {
    use gossil::gest_model::tag::Model;

    mod new {
        use super::*;

        #[test]
        fn it_creates_a_tag_with_label() {
            let tag = Model::new("blocked");

            assert_eq!(tag.label(), "blocked");
        }
    }
}

mod primitives {
    mod author_type {
        use gossil::gest_model::primitives::AuthorType as Primitive;

        mod display {
            use super::*;

            #[test]
            fn it_formats_agent() {
                assert_eq!(Primitive::Agent.to_string(), "agent");
            }

            #[test]
            fn it_formats_human() {
                assert_eq!(Primitive::Human.to_string(), "human");
            }
        }

        mod from_str {
            use super::*;

            #[test]
            fn it_parses_case_insensitively() {
                assert_eq!("AGENT".parse::<Primitive>().unwrap(), Primitive::Agent);
                assert_eq!("Human".parse::<Primitive>().unwrap(), Primitive::Human);
            }

            #[test]
            fn it_rejects_invalid() {
                assert!("robot".parse::<Primitive>().is_err());
            }
        }
    }

    mod entity_type {
        use gossil::gest_model::primitives::EntityType as Primitive;

        mod display {
            use super::*;

            #[test]
            fn it_formats_artifact() {
                assert_eq!(Primitive::Artifact.to_string(), "artifact");
            }

            #[test]
            fn it_formats_iteration() {
                assert_eq!(Primitive::Iteration.to_string(), "iteration");
            }

            #[test]
            fn it_formats_task() {
                assert_eq!(Primitive::Task.to_string(), "task");
            }
        }

        mod from_str {
            use super::*;

            #[test]
            fn it_parses_case_insensitively() {
                assert_eq!(
                    "ARTIFACT".parse::<Primitive>().unwrap(),
                    Primitive::Artifact
                );
                assert_eq!("Task".parse::<Primitive>().unwrap(), Primitive::Task);
            }

            #[test]
            fn it_rejects_invalid() {
                assert!("widget".parse::<Primitive>().is_err());
            }
        }
    }

    mod id {
        use gossil::gest_model::primitives::Id as Primitive;
        use libsql::Value;

        mod from_id_for_value {
            use super::*;

            #[test]
            fn it_converts_to_text_value() {
                let id: Primitive = "zyxwvutsrqponmlkzyxwvutsrqponmlk".parse().unwrap();
                let value = Value::from(id);
                assert_eq!(
                    value,
                    Value::from("zyxwvutsrqponmlkzyxwvutsrqponmlk".to_string())
                );
            }
        }

        mod from_str {
            use super::*;

            #[test]
            fn it_parses_valid_id() {
                let id: Primitive = "zyxwvutsrqponmlkzyxwvutsrqponmlk".parse().unwrap();
                assert_eq!(id.to_string(), "zyxwvutsrqponmlkzyxwvutsrqponmlk");
            }

            #[test]
            fn it_rejects_digits() {
                let result = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz1".parse::<Primitive>();
                assert!(result.is_err());
            }

            #[test]
            fn it_rejects_out_of_range_chars() {
                let result = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzja".parse::<Primitive>();
                assert!(result.is_err());
            }

            #[test]
            fn it_rejects_too_long() {
                let result = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".parse::<Primitive>();
                assert!(result.is_err());
            }

            #[test]
            fn it_rejects_too_short() {
                let result = "zzz".parse::<Primitive>();
                assert!(result.is_err());
            }
        }

        mod new {
            use super::*;

            #[test]
            fn it_generates_valid_ids() {
                let id = Primitive::new();
                let s = id.to_string();
                assert_eq!(s.len(), 32);
                assert!(s.chars().all(|c| ('k'..='z').contains(&c)));
            }
        }

        mod validate_prefix {
            use super::*;

            #[test]
            fn it_accepts_full_id() {
                assert!(Primitive::validate_prefix("zyxwvutsrqponmlkzyxwvutsrqponmlk").is_ok());
            }

            #[test]
            fn it_accepts_short_prefix() {
                assert_eq!(Primitive::validate_prefix("zyxw").unwrap(), "zyxw");
            }

            #[test]
            fn it_accepts_single_char() {
                assert!(Primitive::validate_prefix("k").is_ok());
            }

            #[test]
            fn it_rejects_chars_below_range() {
                assert!(Primitive::validate_prefix("kkkj").is_err());
            }

            #[test]
            fn it_rejects_digits() {
                assert!(Primitive::validate_prefix("kkkk1").is_err());
            }

            #[test]
            fn it_rejects_dots() {
                assert!(Primitive::validate_prefix("..").is_err());
            }

            #[test]
            fn it_rejects_empty() {
                assert!(Primitive::validate_prefix("").is_err());
            }

            #[test]
            fn it_rejects_path_separator() {
                assert!(Primitive::validate_prefix("../etc").is_err());
            }

            #[test]
            fn it_rejects_too_long() {
                let long = "k".repeat(33);
                assert!(Primitive::validate_prefix(&long).is_err());
            }

            #[test]
            fn it_rejects_uppercase() {
                assert!(Primitive::validate_prefix("kkkK").is_err());
            }
        }

        mod display {
            use super::*;

            #[test]
            fn it_formats_as_the_encoded_string() {
                let id: Primitive = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".parse().unwrap();
                assert_eq!(id.to_string(), "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
            }
        }

        mod roundtrip {
            use super::*;

            #[test]
            fn it_roundtrips_through_string() {
                let id = Primitive::new();
                let s = id.to_string();
                let parsed: Primitive = s.parse().unwrap();
                assert_eq!(id, parsed);
            }
        }

        mod short {
            use super::*;

            #[test]
            fn it_returns_first_eight_characters() {
                let id: Primitive = "zyxwvutsrqponmlkzyxwvutsrqponmlk".parse().unwrap();
                assert_eq!(id.short(), "zyxwvuts");
            }
        }
    }

    mod iteration_status {
        use gossil::gest_model::primitives::IterationStatus as Primitive;

        mod display {
            use super::*;

            #[test]
            fn it_formats_active() {
                assert_eq!(Primitive::Active.to_string(), "active");
            }

            #[test]
            fn it_formats_cancelled() {
                assert_eq!(Primitive::Cancelled.to_string(), "cancelled");
            }

            #[test]
            fn it_formats_completed() {
                assert_eq!(Primitive::Completed.to_string(), "completed");
            }
        }

        mod is_terminal {
            use super::*;

            #[test]
            fn it_returns_false_for_active() {
                assert!(!Primitive::Active.is_terminal());
            }

            #[test]
            fn it_returns_true_for_cancelled() {
                assert!(Primitive::Cancelled.is_terminal());
            }

            #[test]
            fn it_returns_true_for_completed() {
                assert!(Primitive::Completed.is_terminal());
            }
        }
    }

    mod priority {
        use gossil::gest_model::primitives::Priority as Primitive;

        mod all {
            use super::*;

            #[test]
            fn it_contains_every_variant_in_rank_order() {
                assert_eq!(
                    Primitive::ALL,
                    &[
                        Primitive::Critical,
                        Primitive::High,
                        Primitive::Medium,
                        Primitive::Low,
                        Primitive::Lowest,
                    ]
                );
            }
        }

        mod display {
            use super::*;

            #[test]
            fn it_formats_critical() {
                assert_eq!(Primitive::Critical.to_string(), "critical");
            }

            #[test]
            fn it_formats_high() {
                assert_eq!(Primitive::High.to_string(), "high");
            }

            #[test]
            fn it_formats_low() {
                assert_eq!(Primitive::Low.to_string(), "low");
            }

            #[test]
            fn it_formats_lowest() {
                assert_eq!(Primitive::Lowest.to_string(), "lowest");
            }

            #[test]
            fn it_formats_medium() {
                assert_eq!(Primitive::Medium.to_string(), "medium");
            }
        }

        mod from_str {
            use super::*;

            #[test]
            fn it_parses_each_label() {
                assert_eq!(
                    "critical".parse::<Primitive>().unwrap(),
                    Primitive::Critical
                );
                assert_eq!("high".parse::<Primitive>().unwrap(), Primitive::High);
                assert_eq!("medium".parse::<Primitive>().unwrap(), Primitive::Medium);
                assert_eq!("low".parse::<Primitive>().unwrap(), Primitive::Low);
                assert_eq!("lowest".parse::<Primitive>().unwrap(), Primitive::Lowest);
            }

            #[test]
            fn it_parses_case_insensitively() {
                assert_eq!(
                    "CRITICAL".parse::<Primitive>().unwrap(),
                    Primitive::Critical
                );
                assert_eq!(
                    "Critical".parse::<Primitive>().unwrap(),
                    Primitive::Critical
                );
                assert_eq!(
                    "critical".parse::<Primitive>().unwrap(),
                    Primitive::Critical
                );
            }

            #[test]
            fn it_rejects_an_empty_string() {
                assert!("".parse::<Primitive>().is_err());
            }

            #[test]
            fn it_rejects_unknown_labels() {
                let err = "urgent".parse::<Primitive>().unwrap_err();
                assert!(err.contains("urgent"));
            }

            #[test]
            fn it_round_trips_through_string() {
                for priority in Primitive::ALL {
                    let rendered = priority.to_string();
                    assert_eq!(rendered.parse::<Primitive>().unwrap(), *priority);
                }
            }
        }

        mod try_from_u8 {
            use super::*;

            #[test]
            fn it_accepts_values_in_range() {
                assert_eq!(Primitive::try_from(0u8).unwrap(), Primitive::Critical);
                assert_eq!(Primitive::try_from(1u8).unwrap(), Primitive::High);
                assert_eq!(Primitive::try_from(2u8).unwrap(), Primitive::Medium);
                assert_eq!(Primitive::try_from(3u8).unwrap(), Primitive::Low);
                assert_eq!(Primitive::try_from(4u8).unwrap(), Primitive::Lowest);
            }

            #[test]
            fn it_rejects_the_max_u8() {
                assert!(Primitive::try_from(255u8).is_err());
            }

            #[test]
            fn it_rejects_values_just_out_of_range() {
                assert!(Primitive::try_from(5u8).is_err());
            }

            #[test]
            fn it_round_trips_through_u8() {
                for priority in Primitive::ALL {
                    let byte: u8 = (*priority).into();
                    assert_eq!(Primitive::try_from(byte).unwrap(), *priority);
                }
            }
        }
    }

    mod relationship_type {
        use gossil::gest_model::primitives::RelationshipType as Primitive;

        mod display {
            use super::*;

            #[test]
            fn it_formats_blocked_by() {
                assert_eq!(Primitive::BlockedBy.to_string(), "blocked-by");
            }

            #[test]
            fn it_formats_blocks() {
                assert_eq!(Primitive::Blocks.to_string(), "blocks");
            }

            #[test]
            fn it_formats_child_of() {
                assert_eq!(Primitive::ChildOf.to_string(), "child-of");
            }

            #[test]
            fn it_formats_parent_of() {
                assert_eq!(Primitive::ParentOf.to_string(), "parent-of");
            }

            #[test]
            fn it_formats_relates_to() {
                assert_eq!(Primitive::RelatesTo.to_string(), "relates-to");
            }
        }

        mod inverse {
            use super::*;

            #[test]
            fn it_inverts_blocked_by_to_blocks() {
                assert_eq!(Primitive::BlockedBy.inverse(), Primitive::Blocks);
            }

            #[test]
            fn it_inverts_blocks_to_blocked_by() {
                assert_eq!(Primitive::Blocks.inverse(), Primitive::BlockedBy);
            }

            #[test]
            fn it_inverts_child_of_to_parent_of() {
                assert_eq!(Primitive::ChildOf.inverse(), Primitive::ParentOf);
            }

            #[test]
            fn it_inverts_parent_of_to_child_of() {
                assert_eq!(Primitive::ParentOf.inverse(), Primitive::ChildOf);
            }

            #[test]
            fn it_keeps_relates_to_symmetric() {
                assert_eq!(Primitive::RelatesTo.inverse(), Primitive::RelatesTo);
            }
        }
    }

    mod task_status {
        use gossil::gest_model::primitives::TaskStatus as Primitive;

        mod display {
            use super::*;

            #[test]
            fn it_formats_cancelled() {
                assert_eq!(Primitive::Cancelled.to_string(), "cancelled");
            }

            #[test]
            fn it_formats_done() {
                assert_eq!(Primitive::Done.to_string(), "done");
            }

            #[test]
            fn it_formats_in_progress() {
                assert_eq!(Primitive::InProgress.to_string(), "in-progress");
            }

            #[test]
            fn it_formats_open() {
                assert_eq!(Primitive::Open.to_string(), "open");
            }
        }

        mod is_terminal {
            use super::*;

            #[test]
            fn it_returns_false_for_in_progress() {
                assert!(!Primitive::InProgress.is_terminal());
            }

            #[test]
            fn it_returns_false_for_open() {
                assert!(!Primitive::Open.is_terminal());
            }

            #[test]
            fn it_returns_true_for_cancelled() {
                assert!(Primitive::Cancelled.is_terminal());
            }

            #[test]
            fn it_returns_true_for_done() {
                assert!(Primitive::Done.is_terminal());
            }
        }
    }
}

/// New tests of what the Gossil port adds to or changes in the upstream model.
mod port {
    use super::{gest_source_map, utc_now};
    use gossil::gest_model::{
        artifact, author, entity_tag, iteration, note,
        primitives::{
            AuthorType, EntityType, Id, IterationStatus, Priority, RelationshipType, TaskStatus,
        },
        project, relationship, tag, task,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::str::FromStr;

    const A: &str = "klmnopqrstuvwxyzklmnopqrstuvwxyz";
    const B: &str = "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk";
    const C: &str = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";

    /// Serde gives each enum the wire spelling of the Gest API, which differs
    /// from `Display` only for the in-progress task status (GEST-02).
    #[test]
    fn enums_use_the_upstream_serde_spellings() {
        let cases = [
            (json!(TaskStatus::InProgress), "inprogress"),
            (json!(TaskStatus::Open), "open"),
            (json!(IterationStatus::Completed), "completed"),
            (json!(AuthorType::Agent), "agent"),
            (json!(EntityType::Iteration), "iteration"),
            (json!(RelationshipType::BlockedBy), "blocked-by"),
            (json!(Priority::Lowest), "lowest"),
        ];
        for (encoded, spelling) in cases {
            assert_eq!(encoded, json!(spelling));
        }
        assert_eq!(
            serde_json::from_value::<TaskStatus>(json!("inprogress")).unwrap(),
            TaskStatus::InProgress
        );
        assert!(serde_json::from_value::<TaskStatus>(json!("in-progress")).is_err());
        assert_eq!(
            serde_json::from_value::<RelationshipType>(json!("child-of")).unwrap(),
            RelationshipType::ChildOf
        );
        assert_eq!(
            serde_json::from_value::<Priority>(json!("medium")).unwrap(),
            Priority::Medium
        );
        assert_eq!(TaskStatus::default(), TaskStatus::Open);
        assert_eq!(IterationStatus::default(), IterationStatus::Active);
        assert_eq!(AuthorType::default(), AuthorType::Human);
    }

    /// `FromStr` accepts the three upstream spellings of in progress, and the
    /// error of each parser names the text that it refused.
    #[test]
    fn parsers_accept_upstream_aliases_and_name_refused_text() {
        for text in ["in-progress", "IN_PROGRESS", "InProgress"] {
            assert_eq!(text.parse::<TaskStatus>().unwrap(), TaskStatus::InProgress);
        }
        assert_eq!(
            "paused".parse::<TaskStatus>().unwrap_err(),
            "invalid task status: paused"
        );
        assert_eq!(
            "Paused".parse::<IterationStatus>().unwrap_err(),
            "invalid iteration status: paused"
        );
        assert_eq!(
            "Robot".parse::<AuthorType>().unwrap_err(),
            "invalid author type: robot"
        );
        assert_eq!(
            "Widget".parse::<EntityType>().unwrap_err(),
            "invalid entity type: widget"
        );
        assert_eq!(
            "child_of".parse::<RelationshipType>().unwrap_err(),
            "invalid relationship type: child_of"
        );
        assert_eq!(
            Priority::try_from(9).unwrap_err(),
            "invalid priority value: 9"
        );
        for status in [
            IterationStatus::Active,
            IterationStatus::Cancelled,
            IterationStatus::Completed,
        ] {
            assert_eq!(status.to_string().parse(), Ok(status));
        }
    }

    /// Decoding an identifier is fallible, so a wrong value is an error and never a panic.
    #[test]
    fn identifiers_decode_without_panicking() {
        for bad in [
            json!(""),
            json!("short"),
            json!("jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj"),
            json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkj"),
            json!(null),
            json!(123),
        ] {
            assert!(serde_json::from_value::<Id>(bad).is_err());
        }
        let id = Id::from_str(A).unwrap();
        assert_eq!(json!(id), json!(A));
        assert_eq!(serde_json::from_value::<Id>(json!(A)).unwrap(), id);
        assert_eq!(
            Id::from_str("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzj").unwrap_err(),
            "Id must contain only characters k-z, found 'j'"
        );
        assert_eq!(
            Id::from_str("zzz").unwrap_err(),
            "Id must be exactly 32 characters, got 3"
        );
        assert_eq!(
            Id::validate_prefix(&"k".repeat(33)).unwrap_err(),
            "Id prefix must be at most 32 characters, got 33"
        );
        assert_eq!(
            Id::validate_prefix("kkK").unwrap_err(),
            "Id prefix must contain only characters k-z, found 'K'"
        );
        assert_eq!(
            Id::validate_prefix("").unwrap_err(),
            "Id prefix must not be empty"
        );
    }

    /// The upstream convenience conversions keep their panic for invalid text,
    /// and return the same identifier as `FromStr` for valid text.
    #[test]
    fn convenience_conversions_keep_the_upstream_contract() {
        let id = Id::from(A);
        assert_eq!(Id::from(A.to_owned()), id);
        assert_eq!(String::from(id.clone()), A);
        assert!(std::panic::catch_unwind(|| Id::from("invalid")).is_err());
        assert!(std::panic::catch_unwind(|| Id::from(String::from("invalid"))).is_err());
        let fresh = Id::default();
        assert_ne!(fresh, Id::new());
        assert_eq!(fresh.to_string().parse::<Id>().unwrap(), fresh);
        assert_eq!(fresh.short(), fresh.to_string()[..8]);
    }

    /// Each constructor that reads the clock sets equal creation and change
    /// times between two readings, and a fresh valid identifier.
    #[test]
    fn constructors_read_the_clock_and_make_fresh_identifiers() {
        let before = utc_now();
        let author = author::Model::new("", AuthorType::Agent).with_email("");
        let tag = tag::Model::new("Urgent");
        let project = project::Model::new(PathBuf::from("/root"));
        let link = entity_tag::Model::new(EntityType::Task, Id::from(A), Id::from(B));
        let after = utc_now();
        for created in [
            author.created_at(),
            tag.created_at(),
            project.created_at(),
            link.created_at(),
        ] {
            assert!(*created >= before && *created <= after);
        }
        assert_eq!(author.created_at(), author.updated_at());
        assert_eq!(tag.created_at(), tag.updated_at());
        assert_eq!(author.email().as_deref(), Some(""));
        assert_eq!(author.name(), "");
        assert_eq!(tag.label(), "Urgent");
        assert_eq!(project.root(), &PathBuf::from("/root"));
        assert_eq!(project.archived_at(), &None);
        assert_eq!(link.entity_type(), EntityType::Task);
        assert_eq!(link.entity_id().to_string(), A);
        assert_eq!(link.tag_id().to_string(), B);
        let ids: BTreeSet<String> = [author.id(), tag.id(), project.id()]
            .into_iter()
            .map(Id::to_string)
            .collect();
        assert_eq!(ids.len(), 3);
    }

    /// A project rebuilt from synced parts keeps each part, and serde keeps the root.
    #[test]
    fn project_from_synced_parts_keeps_each_part() {
        let instant = |day: u32| {
            chrono::DateTime::parse_from_rfc3339(&format!("2026-11-{day:02}T00:00:00Z"))
                .unwrap()
                .to_utc()
        };
        let (archived, created, updated) = (Some(instant(20)), instant(18), instant(19));
        let project = project::Model::from_synced_parts(
            Id::from(A),
            PathBuf::from("/scratch/b"),
            archived,
            created,
            updated,
        );
        assert_eq!(project.id().to_string(), A);
        assert_eq!(project.root(), &PathBuf::from("/scratch/b"));
        assert_eq!(project.archived_at(), &archived);
        assert_eq!(project.created_at(), &created);
        assert_eq!(project.updated_at(), &updated);
        let encoded = wire_form(
            &project,
            &["archived_at", "created_at", "id", "root", "updated_at"],
        );
        assert_eq!(encoded["root"], json!("/scratch/b"));
        assert_eq!(
            serde_json::from_value::<project::Model>(encoded).unwrap(),
            project
        );
    }

    /// A validated task record becomes a task model with every field, and the
    /// model writes the Gest serde form.
    #[test]
    fn task_model_comes_from_a_validated_record() {
        use gossil::gest_task_values::{TaskValueInputs, task_from_values};
        let record = task_from_values(TaskValueInputs {
            gest_id: json!(A),
            project_id: json!(B),
            title: json!("Review"),
            priority: json!(255),
            status: json!("IN_PROGRESS"),
            description: json!(""),
            assigned_to: json!(C),
            metadata: json!([null, 1]),
            resolved_at: json!(null),
            created_at: json!("2026-11-18T07:00:00-05:00"),
            updated_at: json!("2026-11-18T12:00:00.5Z"),
        })
        .unwrap();
        let model = task::Model::from(record);
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.project_id().to_string(), B);
        assert_eq!(
            model.assigned_to().as_ref().map(Id::to_string),
            Some(C.into())
        );
        assert_eq!(model.title(), "Review");
        assert_eq!(model.description(), "");
        assert_eq!(model.priority(), Some(255));
        assert_eq!(model.status(), TaskStatus::InProgress);
        assert_eq!(model.metadata(), &json!([null, 1]));
        assert_eq!(model.resolved_at(), &None);
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T12:00:00+00:00");
        assert_eq!(
            model.updated_at().to_rfc3339(),
            "2026-11-18T12:00:00.500+00:00"
        );
        let encoded = serde_json::to_value(&model).unwrap();
        assert_eq!(encoded["status"], json!("inprogress"));
        assert_eq!(encoded["id"], json!(A));
        assert_eq!(
            serde_json::from_value::<task::Model>(encoded).unwrap(),
            model
        );
    }

    /// A terminal task keeps its resolution time through the conversion.
    #[test]
    fn terminal_task_keeps_its_resolution_time() {
        use gossil::gest_task_values::{TaskValueInputs, task_from_values};
        let record = task_from_values(TaskValueInputs {
            gest_id: json!(A),
            project_id: json!(B),
            title: json!(""),
            priority: json!(null),
            status: json!("done"),
            description: json!("d"),
            assigned_to: json!(null),
            metadata: json!({}),
            resolved_at: json!("2026-11-19T00:00:00Z"),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        })
        .unwrap();
        let model = task::Model::from(record);
        assert_eq!(model.status(), TaskStatus::Done);
        assert_eq!(model.priority(), None);
        assert_eq!(model.assigned_to(), &None);
        assert_eq!(
            model.resolved_at().map(|t| t.to_rfc3339()),
            Some("2026-11-19T00:00:00+00:00".into())
        );
    }

    /// A validated artifact record keeps its complete body in the model, and
    /// serde skips the body as upstream does.
    #[test]
    fn artifact_model_keeps_the_body_that_serde_skips() {
        use gossil::gest_artifact_values::{ArtifactValueInputs, artifact_from_values};
        let record = artifact_from_values(ArtifactValueInputs {
            gest_id: json!(A),
            project_id: json!(B),
            title: json!("Spec"),
            body: json!("# Body\n\u{e9}"),
            metadata: json!(null),
            archived_at: json!("2026-11-20T00:00:00Z"),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        })
        .unwrap();
        let model = artifact::Model::from(record);
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.project_id().to_string(), B);
        assert_eq!(model.title(), "Spec");
        assert_eq!(model.body(), "# Body\n\u{e9}");
        assert_eq!(model.metadata(), &Value::Null);
        assert!(model.is_archived());
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T00:00:00+00:00");
        assert_eq!(model.updated_at().to_rfc3339(), "2026-11-19T00:00:00+00:00");
        let encoded = serde_json::to_value(&model).unwrap();
        assert!(encoded.get("body").is_none());
        let decoded: artifact::Model = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.body(), "");
        assert_eq!(decoded.archived_at(), model.archived_at());
        let mut active = serde_json::to_value(&model).unwrap();
        active["archived_at"] = Value::Null;
        assert!(
            !serde_json::from_value::<artifact::Model>(active)
                .unwrap()
                .is_archived()
        );
    }

    /// A validated iteration record becomes an iteration model with every field.
    #[test]
    fn iteration_model_comes_from_a_validated_record() {
        use gossil::gest_iteration_values::{IterationValueInputs, iteration_from_values};
        let record = iteration_from_values(IterationValueInputs {
            gest_id: json!(A),
            project_id: json!(B),
            title: json!("Sprint"),
            status: json!("Completed"),
            description: json!("goals"),
            metadata: json!({"k": 1}),
            completed_at: json!("2026-11-20T00:00:00Z"),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        })
        .unwrap();
        let model = iteration::Model::from(record);
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.project_id().to_string(), B);
        assert_eq!(model.title(), "Sprint");
        assert_eq!(model.status(), IterationStatus::Completed);
        assert_eq!(model.description(), "goals");
        assert_eq!(model.metadata(), &json!({"k": 1}));
        assert_eq!(
            model.completed_at().map(|t| t.to_rfc3339()),
            Some("2026-11-20T00:00:00+00:00".into())
        );
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T00:00:00+00:00");
        assert_eq!(model.updated_at().to_rfc3339(), "2026-11-19T00:00:00+00:00");
        let encoded = wire_form(
            &model,
            &[
                "completed_at",
                "created_at",
                "description",
                "id",
                "metadata",
                "project_id",
                "status",
                "title",
                "updated_at",
            ],
        );
        assert_eq!(encoded["status"], "completed");
        assert_eq!(encoded["id"], A);
    }

    /// A validated author record becomes an author model, with a null email kept as `None`.
    #[test]
    fn author_model_comes_from_a_validated_record() {
        use gossil::gest_author_values::{AuthorValueInputs, author_from_values};
        let record = author_from_values(AuthorValueInputs {
            gest_id: json!(A),
            author_type: json!("AGENT"),
            name: json!("Claude"),
            email: json!(null),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        })
        .unwrap();
        let model = author::Model::from(record);
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.author_type(), AuthorType::Agent);
        assert_eq!(model.name(), "Claude");
        assert_eq!(model.email(), &None);
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T00:00:00+00:00");
        assert_eq!(model.updated_at().to_rfc3339(), "2026-11-19T00:00:00+00:00");
        let encoded = serde_json::to_value(&model).unwrap();
        assert_eq!(encoded["author_type"], "agent");
        assert_eq!(
            serde_json::from_value::<author::Model>(encoded).unwrap(),
            model
        );
    }

    /// A validated tag record becomes a tag model with its exact label.
    #[test]
    fn tag_model_comes_from_a_validated_record() {
        use gossil::gest_tag_values::{TagValueInputs, tag_from_values};
        let record = tag_from_values(TagValueInputs {
            gest_id: json!(A),
            label: json!("Urgent"),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        })
        .unwrap();
        let model = tag::Model::from(record);
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.label(), "Urgent");
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T00:00:00+00:00");
        assert_eq!(model.updated_at().to_rfc3339(), "2026-11-19T00:00:00+00:00");
        let encoded = wire_form(&model, &["created_at", "id", "label", "updated_at"]);
        assert_eq!(encoded["label"], "Urgent");
        assert_eq!(encoded["id"], A);
    }

    /// A validated note record becomes a note model, with an unknown author kept as `None`.
    #[test]
    fn note_model_comes_from_a_validated_record() {
        use gossil::gest_note_values::{NoteValueInputs, note_from_values};
        let inputs = |author: Value| NoteValueInputs {
            gest_id: json!(A),
            entity_id: json!(B),
            entity_type: json!("ARTIFACT"),
            author_id: author,
            body: json!(""),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        };
        let model = note::Model::from(note_from_values(inputs(json!(C))).unwrap());
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.entity_id().to_string(), B);
        assert_eq!(model.entity_type(), EntityType::Artifact);
        assert_eq!(
            model.author_id().as_ref().map(Id::to_string),
            Some(C.into())
        );
        assert_eq!(model.body(), "");
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T00:00:00+00:00");
        assert_eq!(model.updated_at().to_rfc3339(), "2026-11-19T00:00:00+00:00");
        let keys = [
            "author_id",
            "body",
            "created_at",
            "entity_id",
            "entity_type",
            "id",
            "updated_at",
        ];
        let encoded = wire_form(&model, &keys);
        assert_eq!(encoded["entity_type"], "artifact");
        assert_eq!(encoded["author_id"], C);
        let unknown = note::Model::from(note_from_values(inputs(json!(null))).unwrap());
        assert_eq!(unknown.author_id(), &None);
        assert_eq!(wire_form(&unknown, &keys)["author_id"], Value::Null);
    }

    /// A validated relationship record becomes a model with the same direction.
    #[test]
    fn relationship_model_keeps_the_direction_of_its_record() {
        use gossil::gest_relationship_values::{RelationshipValueInputs, relationship_from_values};
        let record = relationship_from_values(RelationshipValueInputs {
            gest_id: json!(A),
            rel_type: json!("Child-Of"),
            source_id: json!(B),
            source_type: json!("task"),
            target_id: json!(C),
            target_type: json!("ITERATION"),
            created_at: json!("2026-11-18T00:00:00Z"),
            updated_at: json!("2026-11-19T00:00:00Z"),
        })
        .unwrap();
        let model = relationship::Model::from(record);
        assert_eq!(model.id().to_string(), A);
        assert_eq!(model.rel_type(), RelationshipType::ChildOf);
        assert_eq!(model.source_id().to_string(), B);
        assert_eq!(model.source_type(), EntityType::Task);
        assert_eq!(model.target_id().to_string(), C);
        assert_eq!(model.target_type(), EntityType::Iteration);
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-18T00:00:00+00:00");
        assert_eq!(model.updated_at().to_rfc3339(), "2026-11-19T00:00:00+00:00");
        let encoded = wire_form(
            &model,
            &[
                "created_at",
                "id",
                "rel_type",
                "source_id",
                "source_type",
                "target_id",
                "target_type",
                "updated_at",
            ],
        );
        assert_eq!(encoded["rel_type"], "child-of");
        assert_eq!(
            (&encoded["source_id"], &encoded["target_id"]),
            (&json!(B), &json!(C))
        );
        assert_eq!(encoded["target_type"], "iteration");
    }

    /// A validated entity tag record becomes an association model.
    #[test]
    fn entity_tag_model_comes_from_a_validated_record() {
        use gossil::gest_entity_tag_values::{EntityTagValueInputs, entity_tag_from_values};
        let record = entity_tag_from_values(EntityTagValueInputs {
            entity_id: json!(A),
            entity_type: json!("Iteration"),
            tag_id: json!(B),
            created_at: json!("2026-11-18T00:00:00+01:00"),
        })
        .unwrap();
        let model = entity_tag::Model::from(record);
        assert_eq!(model.entity_id().to_string(), A);
        assert_eq!(model.entity_type(), EntityType::Iteration);
        assert_eq!(model.tag_id().to_string(), B);
        assert_eq!(model.created_at().to_rfc3339(), "2026-11-17T23:00:00+00:00");
        let encoded = wire_form(
            &model,
            &["created_at", "entity_id", "entity_type", "tag_id"],
        );
        assert_eq!(encoded["entity_type"], "iteration");
        assert_eq!(
            (&encoded["entity_id"], &encoded["tag_id"]),
            (&json!(A), &json!(B))
        );
    }

    /// Encode `model` with serde, check that the JSON object has exactly the
    /// `keys` of the Gest wire form, and check that it decodes back to `model`.
    ///
    /// # Returns
    ///
    /// The encoded JSON value.
    fn wire_form<M>(model: &M, keys: &[&str]) -> Value
    where
        M: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let encoded = serde_json::to_value(model).unwrap();
        let found: Vec<&str> = encoded
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(found, keys);
        assert_eq!(
            &serde_json::from_value::<M>(encoded.clone()).unwrap(),
            model
        );
        encoded
    }

    /// Return the name of each function in `source`, before its test module,
    /// whose body reads the clock or makes a random identifier through one of
    /// the `calls`, and whose doc comment has no `# Panics` section.
    fn undocumented_panics(source: &str, calls: &[&str]) -> Vec<String> {
        let source = source.split("#[cfg(test)]").next().unwrap_or_default();
        let code = code_only(source);
        let raw: Vec<&str> = source.lines().collect();
        let lines: Vec<&str> = code.lines().collect();
        let mut missing = Vec::new();
        for (start, line) in lines.iter().enumerate() {
            let Some(name) = line
                .split_once("fn ")
                .and_then(|(_, rest)| rest.split_once('('))
                .map(|(name, _)| name.trim())
                .filter(|name| name.chars().all(|c| c.is_alphanumeric() || c == '_'))
            else {
                continue;
            };
            let (mut depth, mut opened, mut body) = (0i32, false, String::new());
            for line in &lines[start..] {
                body.push_str(line);
                for c in line.chars() {
                    match c {
                        '{' => (depth, opened) = (depth + 1, true),
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if opened && depth <= 0 {
                    break;
                }
            }
            if !calls.iter().any(|call| body.contains(call)) {
                continue;
            }
            let documented = raw[..start]
                .iter()
                .rev()
                .take_while(|l| {
                    l.trim_start().starts_with("///") || l.trim_start().starts_with("#[")
                })
                .any(|l| l.trim() == "/// # Panics");
            if !documented {
                missing.push(name.to_owned());
            }
        }
        missing
    }

    /// Each model function that can panic by reading the clock or by making a
    /// random identifier says so in a `# Panics` section, and the check finds a
    /// function that does not.
    #[test]
    fn functions_that_can_panic_document_it() {
        let calls = ["now()", "Id::new()", "Self::new()"];
        let sources = [
            include_str!("../src/gest_model.rs"),
            include_str!("../src/gest_model/artifact.rs"),
            include_str!("../src/gest_model/author.rs"),
            include_str!("../src/gest_model/entity_tag.rs"),
            include_str!("../src/gest_model/iteration.rs"),
            include_str!("../src/gest_model/note.rs"),
            include_str!("../src/gest_model/project.rs"),
            include_str!("../src/gest_model/relationship.rs"),
            include_str!("../src/gest_model/tag.rs"),
            include_str!("../src/gest_model/task.rs"),
            include_str!("../src/gest_model/primitives/author_type.rs"),
            include_str!("../src/gest_model/primitives/entity_type.rs"),
            include_str!("../src/gest_model/primitives/id.rs"),
            include_str!("../src/gest_model/primitives/iteration_status.rs"),
            include_str!("../src/gest_model/primitives/priority.rs"),
            include_str!("../src/gest_model/primitives/relationship_type.rs"),
            include_str!("../src/gest_model/primitives/task_status.rs"),
        ];
        let mut checked = 0;
        for source in sources {
            assert_eq!(undocumented_panics(source, &calls), Vec::<String>::new());
            checked += source.matches("super::now()").count();
        }
        assert!(checked >= 4, "the constructors that read the clock");
        let bare = "/// Make one.\npub fn new() -> Self {\n    Self { at: super::now() }\n}\n";
        assert_eq!(undocumented_panics(bare, &calls), ["new"]);
        let documented = "/// Make one.\n///\n/// # Panics\n///\n/// Clock.\n#[must_use]\npub fn new() -> Self {\n    Self { id: Id::new() }\n}\n";
        assert!(undocumented_panics(documented, &calls).is_empty());
        let quoted = "/// Make one.\npub fn label() -> &'static str {\n    \"now()\"\n}\n";
        assert!(undocumented_panics(quoted, &calls).is_empty());
    }

    /// The input types keep the upstream defaults, which are an unset filter and
    /// a filter from `all` that includes terminal rows.
    #[test]
    fn inputs_keep_the_upstream_defaults() {
        let tasks = task::Filter::all();
        assert!(tasks.all);
        assert!(tasks.assigned_to.is_none() && tasks.status.is_none() && tasks.tag.is_none());
        assert!(!task::Filter::default().all);
        let iterations = iteration::Filter::all();
        assert!(iterations.all && !iterations.has_available);
        assert!(iterations.status.is_none() && iterations.tag.is_none());
        let artifacts = artifact::Filter::default();
        assert!(!artifacts.all && !artifacts.only_archived && artifacts.tag.is_none());

        let new_task: task::New = serde_json::from_value(json!({
            "assigned_to": null, "description": "", "metadata": null,
            "priority": 0, "status": "done", "title": "t"
        }))
        .unwrap();
        assert_eq!(new_task.priority, Some(0));
        assert_eq!(new_task.status, Some(TaskStatus::Done));
        assert_eq!(task::New::default().status, None);
        let patch = task::Patch {
            assigned_to: Some(None),
            priority: Some(None),
            ..task::Patch::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap()["assigned_to"],
            Value::Null
        );
        assert!(task::Patch::default().assigned_to.is_none());
        let artifact_new = artifact::New::default();
        assert!(artifact_new.body.is_empty() && artifact_new.metadata.is_none());
        assert!(artifact::Patch::default().body.is_none());
        assert!(iteration::New::default().title.is_empty());
        assert!(iteration::Patch::default().status.is_none());
        let note_new: note::New =
            serde_json::from_value(json!({"author_id": A, "body": "b"})).unwrap();
        assert_eq!(note_new.author_id, Some(Id::from(A)));
        assert!(
            serde_json::from_value::<note::New>(json!({"author_id": "bad", "body": ""})).is_err()
        );
        assert!(note::Patch::default().body.is_none());
    }

    /// Check the model source map against the shared pin, addendum and hash table.
    ///
    /// # Errors
    ///
    /// Returns the first problem that the check finds. A problem is another pin
    /// or addendum, a cited hash that differs from the table, a test that cites
    /// another commit, or a source that the map does not cite. It is also a
    /// malformed fixture hash, a repeated test, an empty assertion list, or a
    /// count that differs from the list.
    fn check_model_map(map: &Value) -> Result<(), String> {
        let text = |v: &Value, what: &str| -> Result<String, String> {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{what} is not a string"))
        };
        if map["source_pin"] != gest_source_map::SOURCE_PIN {
            return Err("source_pin differs".into());
        }
        if map["governing_addendum_sha256"] != gest_source_map::ADDENDUM_SHA256 {
            return Err("addendum differs".into());
        }
        let mut cited = BTreeSet::new();
        let mut excluded = BTreeSet::new();
        for (key, set) in [("sources", &mut cited), ("excluded_sources", &mut excluded)] {
            for row in map[key].as_array().ok_or("sources are not an array")? {
                let path = text(&row["path"], "path")?;
                let hash = text(&row["sha256"], "sha256")?;
                if hash != gest_source_map::pinned(&path)? {
                    return Err(format!("{path} records hash {hash}"));
                }
                if !set.insert(path.clone()) {
                    return Err(format!("{path} is cited twice"));
                }
            }
        }
        let mut ids = BTreeSet::new();
        for (key, allowed) in [("inherited_tests", &cited), ("excluded_tests", &excluded)] {
            for row in map[key].as_array().ok_or("tests are not an array")? {
                let id = text(&row["test_id"], "test_id")?;
                let rest = id
                    .strip_prefix(&format!("{}:", gest_source_map::SOURCE_PIN))
                    .ok_or_else(|| format!("{id} cites another commit"))?;
                let path = rest.split(':').next().unwrap_or_default();
                if !allowed.contains(path) {
                    return Err(format!("{id} cites a source outside {key}"));
                }
                let hash = text(&row["fixture_sha256"], "fixture_sha256")?;
                if hash.len() != 64 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                {
                    return Err(format!("{id} has a malformed fixture hash"));
                }
                if !ids.insert(id.clone()) {
                    return Err(format!("{id} appears twice"));
                }
                if key == "inherited_tests" {
                    let assertions = row["assertion_ids"]
                        .as_array()
                        .ok_or("assertion_ids is not an array")?;
                    let prefix = format!("{}:{path}:", gest_source_map::SOURCE_PIN);
                    if assertions.is_empty()
                        || !assertions
                            .iter()
                            .all(|a| a.as_str().is_some_and(|a| a.starts_with(&prefix)))
                    {
                        return Err(format!("{id} has a bad assertion list"));
                    }
                }
            }
        }
        if map["exact_inherited_tests_ported"]
            != map["inherited_tests"].as_array().map_or(0, Vec::len)
        {
            return Err("exact_inherited_tests_ported differs from the list".into());
        }
        Ok(())
    }

    /// Return `source` with each comment, string literal and character literal
    /// made blank, so that braces and names inside them do not count. Block
    /// comments do not nest here, and raw strings are not blanked.
    fn code_only(source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(source.len());
        let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let quote = match c {
                '/' if chars.get(i + 1) == Some(&'/') => {
                    while i < chars.len() && chars[i] != '\n' {
                        out.push(' ');
                        i += 1;
                    }
                    continue;
                }
                '/' if chars.get(i + 1) == Some(&'*') => {
                    while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                        out.push(blank(chars[i]));
                        i += 1;
                    }
                    let end = (i + 2).min(chars.len());
                    out.extend(std::iter::repeat_n(' ', end - i));
                    i = end;
                    continue;
                }
                '"' => '"',
                '\'' if chars.get(i + 1) == Some(&'\\') || chars.get(i + 2) == Some(&'\'') => '\'',
                _ => {
                    out.push(c);
                    i += 1;
                    continue;
                }
            };
            out.push(' ');
            i += 1;
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if let Some(&next) = chars.get(i) {
                    out.push(blank(next));
                    i += 1;
                }
            }
            out.push(' ');
            i += 1;
        }
        out
    }

    /// Return the body span of the item that starts with `head` at brace depth
    /// zero inside `span` of `code`. The `head` ends with its opening brace.
    fn child_body(
        code: &str,
        span: std::ops::Range<usize>,
        head: &str,
    ) -> Option<std::ops::Range<usize>> {
        let bytes = code.as_bytes();
        let mut depth = 0usize;
        for i in span.clone() {
            let starts_word =
                i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            if depth == 0 && starts_word && bytes[i..span.end].starts_with(head.as_bytes()) {
                let open = i + head.len() - 1;
                let mut inner = 0usize;
                for (j, b) in bytes.iter().enumerate().take(span.end).skip(open) {
                    match b {
                        b'{' => inner += 1,
                        b'}' => {
                            inner -= 1;
                            if inner == 0 {
                                return Some(open + 1..j);
                            }
                        }
                        _ => {}
                    }
                }
                return None;
            }
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth = depth.checked_sub(1)?,
                _ => {}
            }
        }
        None
    }

    /// Report whether `path` names a test function inside exactly that chain of
    /// nested modules of `code`, the output of [`code_only`].
    fn port_resolves(code: &str, path: &str) -> bool {
        let segments: Vec<&str> = path.split("::").collect();
        let Some((leaf, modules)) = segments.split_last() else {
            return false;
        };
        let mut span = 0..code.len();
        for module in modules {
            match child_body(code, span, &format!("mod {module} {{")) {
                Some(body) => span = body,
                None => return false,
            }
        }
        child_body(code, span, &format!("fn {leaf}() {{")).is_some()
    }

    /// The port check follows module nesting. It refuses a test named under a
    /// sibling or missing module, and ignores braces and names in comments and
    /// literals.
    #[test]
    fn port_check_refuses_a_test_outside_its_modules() {
        let source = include_str!("gest_model.rs");
        let code = code_only(source);
        assert!(port_resolves(
            &code,
            "primitives::id::validate_prefix::it_rejects_dots"
        ));
        let stale = [
            "primitives::id::from_str::it_rejects_dots",
            "primitives::task_status::display::it_formats_active",
            "id::from_str::it_parses_valid_id",
            "primitives::from_str::it_parses_valid_id",
            "primitives::id::from_str",
        ];
        for path in stale {
            let (modules, leaf) = path.rsplit_once("::").unwrap();
            let flat = modules
                .split("::")
                .all(|m| source.contains(&format!("mod {m} {{")))
                && source.contains(&format!("fn {leaf}() {{"));
            assert!(flat || leaf == "from_str", "{path} is not a sibling mutant");
            assert!(!port_resolves(&code, path), "{path}");
        }
        let id = code_only(include_str!("../src/gest_model/primitives/id.rs"));
        assert!(port_resolves(
            &id,
            "tests::encode_decode::it_encodes_ff_bytes_as_k"
        ));
        assert!(!port_resolves(
            &id,
            "tests::display::it_encodes_ff_bytes_as_k"
        ));
        let literal = "mod a { fn f() { let s = \"}\"; let c = '}'; } }";
        assert!(port_resolves(&code_only(literal), "a::f"));
        let hidden = "mod a {}\n// mod b { fn g() {\nconst S: &str = \"mod c { fn h() { }\";";
        assert!(!port_resolves(&code_only(hidden), "b::g"));
        assert!(!port_resolves(&code_only(hidden), "c::h"));
        let block = "mod a { /* } mod d { fn k() { */ fn f() {} }";
        assert!(port_resolves(&code_only(block), "a::f"));
        assert!(!port_resolves(&code_only(block), "a::d::k"));
        assert!(!port_resolves(&code_only(literal), ""));
    }

    /// The source map cites each ported upstream file with its pinned hash, and
    /// names a port that exists for each of the 77 inherited model tests.
    #[test]
    fn source_map_names_a_port_for_every_inherited_test() {
        let map: Value =
            serde_json::from_str(include_str!("fixtures/gest_model/source-map.json")).unwrap();
        check_model_map(&map).unwrap();
        let ports = [
            ("tests/gest_model.rs", include_str!("gest_model.rs")),
            (
                "src/gest_model/primitives/id.rs",
                include_str!("../src/gest_model/primitives/id.rs"),
            ),
        ];
        let codes = ports.map(|(file, source)| (file, code_only(source)));
        let tests = map["inherited_tests"].as_array().unwrap();
        assert_eq!(tests.len(), 77);
        let mut per_file = std::collections::BTreeMap::new();
        let mut paths = BTreeSet::new();
        for row in tests {
            let id = row["test_id"].as_str().unwrap();
            let file = id.split(':').nth(1).unwrap();
            *per_file.entry(file).or_insert(0) += 1;
            let leaf = id.rsplit(':').next().unwrap();
            let port_file = row["port"]["file"].as_str().unwrap();
            let port_path = row["port"]["path"].as_str().unwrap();
            assert!(paths.insert((port_file, port_path)), "{port_path} twice");
            let (_, code) = codes.iter().find(|(f, _)| *f == port_file).unwrap();
            assert_eq!(port_path.rsplit("::").next(), Some(leaf), "{id}");
            assert!(port_resolves(code, port_path), "{port_path}");
        }
        let expected = [
            ("src/store/model/author.rs", 3),
            ("src/store/model/primitives/author_type.rs", 4),
            ("src/store/model/primitives/entity_type.rs", 5),
            ("src/store/model/primitives/id.rs", 24),
            ("src/store/model/primitives/iteration_status.rs", 6),
            ("src/store/model/primitives/priority.rs", 15),
            ("src/store/model/primitives/relationship_type.rs", 10),
            ("src/store/model/primitives/task_status.rs", 8),
            ("src/store/model/project.rs", 1),
            ("src/store/model/tag.rs", 1),
        ];
        assert_eq!(per_file.into_iter().collect::<Vec<_>>(), expected);
        let sources: Vec<&str> = map["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["path"].as_str().unwrap())
            .collect();
        assert_eq!(sources.len(), 18);
        for row in map["sources"].as_array().unwrap() {
            let production = row["production"].as_str().unwrap();
            let upstream = row["path"].as_str().unwrap();
            let expected = match upstream {
                // The primitives module is inline in `src/gest_model.rs`.
                "src/store/model/primitives.rs" => "src/gest_model.rs".to_owned(),
                other => other.replace("src/store/model", "src/gest_model"),
            };
            assert_eq!(production, expected);
        }
        let excluded = map["excluded_tests"].as_array().unwrap();
        assert_eq!(excluded.len(), 1);
        assert!(
            excluded[0]["test_id"]
                .as_str()
                .unwrap()
                .contains("project_workspace.rs")
        );
    }

    /// Every model test that a converter map leaves as a later obligation is a
    /// test that this map ports.
    #[test]
    fn source_map_ports_the_later_obligations_of_the_converter_maps() {
        let map: Value =
            serde_json::from_str(include_str!("fixtures/gest_model/source-map.json")).unwrap();
        let ported: BTreeSet<&str> = map["inherited_tests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["test_id"].as_str().unwrap())
            .collect();
        let converters = [
            include_str!("fixtures/gest_artifact_values/source-map.json"),
            include_str!("fixtures/gest_entity_tag_values/source-map.json"),
            include_str!("fixtures/gest_iteration_task_values/source-map.json"),
            include_str!("fixtures/gest_iteration_values/source-map.json"),
            include_str!("fixtures/gest_note_values/source-map.json"),
            include_str!("fixtures/gest_project_values/source-map.json"),
            include_str!("fixtures/gest_relationship_values/source-map.json"),
            include_str!("fixtures/gest_tag_values/source-map.json"),
            include_str!("fixtures/gest_task_values/source-map.json"),
        ];
        let mut model_obligations = 0;
        for text in converters {
            let converter: Value = serde_json::from_str(text).unwrap();
            for row in converter["later_exact_api_obligations"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let id = row["test_id"].as_str().unwrap();
                if id.contains(":src/store/model/") {
                    model_obligations += 1;
                    assert!(ported.contains(id), "{id} is not ported");
                }
            }
        }
        assert!(model_obligations > 0);
    }

    /// The map check fails for each kind of stale or malformed copy.
    #[test]
    fn source_map_check_refuses_stale_and_malformed_copies() {
        let map: Value =
            serde_json::from_str(include_str!("fixtures/gest_model/source-map.json")).unwrap();
        type Mutation = fn(&mut Value);
        let mutants: [(&str, Mutation); 9] = [
            ("source_pin differs", |m| m["source_pin"] = json!("0")),
            ("addendum differs", |m| {
                m["governing_addendum_sha256"] = json!("0")
            }),
            ("records hash", |m| {
                m["sources"][0]["sha256"] = json!("0".repeat(64))
            }),
            ("is not a pinned Gest source", |m| {
                m["excluded_sources"][0]["path"] = json!("src/unpinned.rs")
            }),
            ("cites another commit", |m| {
                m["inherited_tests"][0]["test_id"] = json!("0:src/store/model/tag.rs:1:t")
            }),
            ("cites a source outside", |m| {
                let id = m["excluded_tests"][0]["test_id"].clone();
                m["inherited_tests"][0]["test_id"] = id;
            }),
            ("malformed fixture hash", |m| {
                m["inherited_tests"][0]["fixture_sha256"] = json!("A".repeat(64))
            }),
            ("appears twice", |m| {
                let first = m["inherited_tests"][0].clone();
                m["inherited_tests"].as_array_mut().unwrap().push(first);
            }),
            ("bad assertion list", |m| {
                m["inherited_tests"][0]["assertion_ids"] = json!([])
            }),
        ];
        for (reason, mutate) in mutants {
            let mut copy = map.clone();
            mutate(&mut copy);
            let error = check_model_map(&copy).unwrap_err();
            assert!(error.contains(reason), "{reason}: {error}");
        }
        let mut copy = map.clone();
        copy["exact_inherited_tests_ported"] = json!(0);
        assert!(
            check_model_map(&copy)
                .unwrap_err()
                .contains("differs from the list")
        );
        let mut copy = map.clone();
        let first = copy["sources"][0].clone();
        copy["sources"].as_array_mut().unwrap().push(first);
        assert!(check_model_map(&copy).unwrap_err().contains("cited twice"));
    }
}
