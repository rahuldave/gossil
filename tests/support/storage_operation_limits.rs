//! Behavior tests of complete file operation limits and their saved declaration.
use super::*;
use crate::artifact::ParseLimits;
use crate::storage::Limits;
use crate::storage::command_codec::CodecWork;

/// Valid native content ceilings.
fn content() -> Limits {
    Limits {
        max_stored_bytes: 1024,
        max_output_bytes: 2048,
        max_chain_depth: 4,
        max_total_stored_bytes: 8192,
        max_total_decoded_bytes: 16_384,
    }
}
/// Valid parser ceilings, with entries above the codec depth cap.
fn parser() -> ParseLimits {
    ParseLimits {
        input_bytes: 4096,
        cards: 64,
        owned_bytes: 512,
        entries: 300,
    }
}
/// Valid filesystem ceilings.
fn filesystem() -> FileSystemLimits {
    FileSystemLimits {
        path_bytes: 256,
        path_components: 16,
        plan_rows: 32,
        retained_bytes: 4096,
        io_buffer_bytes: 1024,
        directory_entries: 64,
    }
}
/// A valid complete limit set.
fn limits() -> FileOperationLimits {
    FileOperationLimits::new(content(), parser(), filesystem(), 777, 8, 9).unwrap()
}

#[test]
fn declared_budgets_keep_every_caller_ceiling() {
    let declared = limits().declared();
    assert_eq!(
        declared,
        DeclaredBudgets {
            stored_bytes: 1024,
            output_bytes: 2048,
            chain_depth: 4,
            total_stored_bytes: 8192,
            total_decoded_bytes: 16_384,
            parse_input_bytes: 4096,
            parse_cards: 64,
            parse_owned_bytes: 512,
            parse_entries: 300,
            path_bytes: 256,
            path_components: 16,
            plan_rows: 32,
            retained_bytes: 4096,
            io_buffer_bytes: 1024,
            directory_entries: 64,
            recipe_bytes: 777,
            closure_objects: 8,
            derived_rows: 9,
        }
    );
    assert_eq!(
        declared.ceilings(),
        [
            1024, 2048, 4, 8192, 16_384, 4096, 64, 512, 300, 256, 16, 32, 4096, 1024, 64, 777, 8, 9
        ]
    );
}

#[test]
fn new_refuses_each_zero_ceiling_and_an_oversized_output() {
    let zero_content = [
        Limits {
            max_stored_bytes: 0,
            ..content()
        },
        Limits {
            max_output_bytes: 0,
            ..content()
        },
        Limits {
            max_chain_depth: 0,
            ..content()
        },
        Limits {
            max_total_stored_bytes: 0,
            ..content()
        },
        Limits {
            max_total_decoded_bytes: 0,
            ..content()
        },
        Limits {
            max_output_bytes: i32::MAX as u32 + 1,
            ..content()
        },
    ];
    for content in zero_content {
        let refused = FileOperationLimits::new(content, parser(), filesystem(), 1, 1, 1)
            .err()
            .unwrap();
        assert_eq!(refused.operation, "complete file limits");
    }
    let zero_parser = [
        ParseLimits {
            input_bytes: 0,
            ..parser()
        },
        ParseLimits {
            cards: 0,
            ..parser()
        },
        ParseLimits {
            owned_bytes: 0,
            ..parser()
        },
        ParseLimits {
            entries: 0,
            ..parser()
        },
    ];
    for parser in zero_parser {
        assert!(FileOperationLimits::new(content(), parser, filesystem(), 1, 1, 1).is_err());
    }
    let zero_filesystem = [
        FileSystemLimits {
            path_bytes: 0,
            ..filesystem()
        },
        FileSystemLimits {
            path_components: 0,
            ..filesystem()
        },
        FileSystemLimits {
            plan_rows: 0,
            ..filesystem()
        },
        FileSystemLimits {
            retained_bytes: 0,
            ..filesystem()
        },
        FileSystemLimits {
            io_buffer_bytes: 0,
            ..filesystem()
        },
        FileSystemLimits {
            directory_entries: 0,
            ..filesystem()
        },
    ];
    for filesystem in zero_filesystem {
        assert!(FileOperationLimits::new(content(), parser(), filesystem, 1, 1, 1).is_err());
    }
    for (recipe, closure, rows) in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        assert!(
            FileOperationLimits::new(content(), parser(), filesystem(), recipe, closure, rows)
                .is_err()
        );
    }
    // The largest supported output ceiling is accepted.
    let largest = Limits {
        max_output_bytes: i32::MAX as u32,
        ..content()
    };
    assert!(FileOperationLimits::new(largest, parser(), filesystem(), 1, 1, 1).is_ok());
}

#[test]
fn saved_declarations_may_narrow_but_never_widen_the_caller() {
    let caller = limits();
    caller.check_saved(&caller.declared()).unwrap();
    let narrower = DeclaredBudgets {
        recipe_bytes: 1,
        derived_rows: 1,
        ..caller.declared()
    };
    caller.check_saved(&narrower).unwrap();
    // Raising any single ceiling by one is refused.
    for index in 0..18 {
        let mut ceilings = caller.declared().ceilings();
        ceilings[index] += 1;
        let saved = budgets(ceilings);
        let refused = caller.check_saved(&saved).unwrap_err();
        assert_eq!(
            refused.operation,
            "saved file operation exceeds caller limits",
            "field {}",
            DeclaredBudgets::FIELDS[index]
        );
    }
    let invalid = DeclaredBudgets {
        plan_rows: 0,
        ..caller.declared()
    };
    let refused = caller.check_saved(&invalid).unwrap_err();
    assert_eq!(refused.operation, "saved file budget invalid");
}

/// Build a declaration from ceilings in the order of [`DeclaredBudgets::FIELDS`].
fn budgets(c: [u64; 18]) -> DeclaredBudgets {
    DeclaredBudgets {
        stored_bytes: c[0],
        output_bytes: c[1] as u32,
        chain_depth: c[2] as u32,
        total_stored_bytes: c[3],
        total_decoded_bytes: c[4],
        parse_input_bytes: c[5] as usize,
        parse_cards: c[6] as usize,
        parse_owned_bytes: c[7] as usize,
        parse_entries: c[8] as usize,
        path_bytes: c[9] as usize,
        path_components: c[10] as usize,
        plan_rows: c[11] as usize,
        retained_bytes: c[12],
        io_buffer_bytes: c[13] as usize,
        directory_entries: c[14] as usize,
        recipe_bytes: c[15] as usize,
        closure_objects: c[16] as usize,
        derived_rows: c[17] as usize,
    }
}

#[test]
fn codec_and_root_limits_derive_from_the_caller_ceilings() {
    let caller = limits();
    assert_eq!(
        caller.codec(),
        CodecLimits {
            bytes: 777,
            nodes: 512,
            depth: 127
        }
    );
    let shallow = FileOperationLimits::new(
        content(),
        ParseLimits {
            entries: 7,
            ..parser()
        },
        filesystem(),
        777,
        8,
        9,
    )
    .unwrap();
    assert_eq!(shallow.codec().depth, 7);
    let root = caller.root();
    assert_eq!(
        (
            root.content.max_total_decoded_bytes,
            root.parser.input_bytes,
            root.parser.entries
        ),
        (16_384, 4096, 300)
    );
}

#[test]
fn saved_declarations_decode_as_closed_records() {
    let declared = limits().declared();
    let mut work = CodecWork::new(limits().codec()).unwrap();
    let bytes = work.encode_record(&declared).unwrap();
    let decoded: DeclaredBudgets = work
        .closed_record(&bytes, &DeclaredBudgets::FIELDS)
        .unwrap();
    assert_eq!(decoded, declared);

    let mut value = serde_json::to_value(&declared).unwrap();
    let object = value.as_object_mut().unwrap();
    object.insert("extra".into(), 1.into());
    let extra = serde_json::to_vec(&value).unwrap();
    let mut work = CodecWork::new(limits().codec()).unwrap();
    assert!(
        work.closed_record::<DeclaredBudgets>(&extra, &DeclaredBudgets::FIELDS)
            .unwrap_err()
            .to_string()
            .contains("unknown closed local record field")
    );
    // A missing field and a field of the wrong type are refused too.
    let mut missing = serde_json::to_value(&declared).unwrap();
    missing.as_object_mut().unwrap().remove("plan_rows");
    let mut work = CodecWork::new(limits().codec()).unwrap();
    assert!(
        work.closed_record::<DeclaredBudgets>(
            &serde_json::to_vec(&missing).unwrap(),
            &DeclaredBudgets::FIELDS
        )
        .unwrap_err()
        .to_string()
        .contains(r#"missing closed local record field "plan_rows""#)
    );
    for wrong in [serde_json::json!(4_294_967_296_u64), serde_json::json!("1")] {
        let mut value = serde_json::to_value(&declared).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("output_bytes".into(), wrong);
        let mut work = CodecWork::new(limits().codec()).unwrap();
        assert!(
            work.closed_record::<DeclaredBudgets>(
                &serde_json::to_vec(&value).unwrap(),
                &DeclaredBudgets::FIELDS
            )
            .unwrap_err()
            .to_string()
            .starts_with("invalid closed local record: ")
        );
    }
}
