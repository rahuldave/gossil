//! Some tests replay the recorded native diagnostics, and separate tests check the new
//! finite-input controls.
use gossil::history::{Checkin, Cherrypick, Direction, Error, Graph, Limits, Options, ParentLink};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
/// Owned metadata from the immutable native capture.
struct Node {
    /// Captured rid value.
    rid: i32,
    /// Captured branch value.
    branch: String,
    /// Captured mtime value.
    mtime: f64,
}
#[derive(Deserialize)]
/// Captured native parent row.
struct Link {
    /// Captured parent value.
    parent: i32,
    /// Captured child value.
    child: i32,
    /// Captured primary value.
    primary: bool,
}
#[derive(Deserialize)]
/// Captured native cherrypick row.
struct Cherry {
    /// Captured parent value.
    parent: i32,
    /// Captured child value.
    child: i32,
    /// Captured excluded value.
    excluded: bool,
}
#[derive(Debug, Deserialize)]
/// Native argv options, status and parsed diagnostic result.
struct Case {
    /// Captured kind value.
    kind: String,
    /// Captured start value.
    start: i32,
    /// Captured end value.
    end: i32,
    /// Captured flags value.
    flags: Vec<String>,
    /// Captured returncode value.
    returncode: i32,
    /// Captured path value.
    path: Vec<i32>,
    /// Captured pivot value.
    pivot: Option<i32>,
}
#[derive(Deserialize)]
/// Immutable native capture deserialization boundary.
struct Fixture {
    /// Captured nodes value.
    nodes: Vec<Node>,
    /// Captured links value.
    links: Vec<Link>,
    /// Captured cherrypicks value.
    cherrypicks: Vec<Cherry>,
    /// Captured cases value.
    cases: Vec<Case>,
}
/// Supply finite budgets large enough for every small graph control.
fn limits() -> Limits {
    Limits {
        checkins: 100,
        links: 300,
        branch_bytes: 1000,
        search_nodes: 1000,
        row_visits: 2000,
    }
}
/// Search both directions through merge links without hidden nodes or weighting.
fn options() -> Options<'static> {
    Options {
        primary_only: false,
        direction: Direction::Both,
        hidden: &[],
        branch_cost: 0,
    }
}
/// Extract the returned RID sequence for exact native-path assertions.
fn ids(path: &gossil::history::Path) -> Vec<i32> {
    path.steps.iter().map(|s| s.rid).collect()
}
/// Build six independent trunk check-ins for focused edge and budget controls.
fn small_nodes() -> Vec<Checkin<'static>> {
    (1..=6)
        .map(|rid| Checkin {
            rid,
            branch: b"trunk",
            mtime: f64::from(rid),
        })
        .collect()
}
/// Construct a primary parent row while preserving caller insertion order.
fn link(parent: i32, child: i32) -> ParentLink {
    ParentLink {
        parent,
        child,
        primary: true,
    }
}

#[test]
/// Captured native diagnostics preserve paths and pivots.
fn captured_native_diagnostics_preserve_paths_and_pivots() {
    let f: Fixture = serde_json::from_str(include_str!("fixtures/history/native.json")).unwrap();
    let nodes: Vec<_> = f
        .nodes
        .iter()
        .map(|n| Checkin {
            rid: n.rid,
            branch: n.branch.as_bytes(),
            mtime: n.mtime,
        })
        .collect();
    let links: Vec<_> = f
        .links
        .iter()
        .map(|l| ParentLink {
            parent: l.parent,
            child: l.child,
            primary: l.primary,
        })
        .collect();
    let cherries: Vec<_> = f
        .cherrypicks
        .iter()
        .map(|l| Cherrypick {
            parent: l.parent,
            child: l.child,
            excluded: l.excluded,
        })
        .collect();
    // The replayed rows must keep the order in which native SQLite returned them.
    let raw: Value = serde_json::from_str(include_str!("fixtures/history/native.json")).unwrap();
    for node in &f.nodes {
        let rows = &raw["query_rows"][node.rid.to_string()];
        let column = |key: &str| -> Vec<(i64, bool)> {
            rows[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| (row[0].as_i64().unwrap(), row[1].as_i64().unwrap() != 0))
                .collect()
        };
        let children: Vec<_> = f
            .links
            .iter()
            .filter(|l| l.parent == node.rid)
            .map(|l| (i64::from(l.child), l.primary))
            .collect();
        let parents: Vec<_> = f
            .links
            .iter()
            .filter(|l| l.child == node.rid)
            .map(|l| (i64::from(l.parent), l.primary))
            .collect();
        assert_eq!(children, column("children"), "children of {}", node.rid);
        assert_eq!(parents, column("parents"), "parents of {}", node.rid);
    }
    let graph = Graph::new(&nodes, &links, &cherries, limits()).unwrap();
    assert_eq!(f.cases.len(), 25);
    for c in f.cases {
        if c.kind == "shortest" {
            let mut o = options();
            o.primary_only = c.flags.iter().any(|f| f == "--no-merge");
            if c.flags.iter().any(|f| f == "--one-way") {
                o.direction = Direction::Forward;
            }
            if let Some(i) = c.flags.iter().position(|f| f == "--branch-cost") {
                o.branch_cost = c.flags[i + 1].parse().unwrap();
            }
            let result = graph.shortest(c.start, c.end, o).unwrap();
            assert_eq!(result.is_some(), c.returncode == 0, "{c:?}");
            assert_eq!(
                result.as_ref().map(ids).unwrap_or_default(),
                c.path,
                "{c:?}"
            );
        } else {
            let result = graph.common_ancestor(c.start, c.end).unwrap();
            if c.start == c.end {
                // Native returns the RID before initializing diagnostic global state.
                // Retain that raw empty diagnostic; the owned API fixes stale state.
                assert!(c.path.is_empty());
                let result = result.unwrap();
                assert_eq!(result.pivot, c.start);
                assert_eq!(
                    result.steps.iter().map(|s| s.rid).collect::<Vec<_>>(),
                    [c.start]
                );
            } else {
                assert_eq!(result.as_ref().map(|a| a.pivot), c.pivot, "{c:?}");
                assert_eq!(
                    result
                        .map(|a| a.steps.iter().map(|s| s.rid).collect::<Vec<_>>())
                        .unwrap_or_default(),
                    c.path,
                    "{c:?}"
                );
            }
        }
    }
}

#[test]
/// A search cannot go backwards after a forward step.
fn cannot_go_backwards_after_a_forward_step() {
    let n = small_nodes();
    let graph = Graph::new(&n, &[link(1, 3), link(2, 3)], &[], limits()).unwrap();
    assert!(graph.shortest(1, 2, options()).unwrap().is_none());
    assert_eq!(
        ids(&graph.shortest(3, 2, options()).unwrap().unwrap()),
        [3, 2]
    );
}

#[test]
/// Hidden nodes change bisect counts but remain on the path.
fn hidden_nodes_change_bisect_counts_but_remain_on_path() {
    let n = small_nodes();
    let graph = Graph::new(
        &n,
        &[link(1, 2), link(2, 3), link(3, 4), link(4, 5)],
        &[],
        limits(),
    )
    .unwrap();
    let p = graph
        .shortest(
            1,
            5,
            Options {
                hidden: &[1, 3],
                ..options()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(ids(&p), [1, 2, 3, 4, 5]);
    assert!(!p.steps[0].hidden); // native initial node is never marked hidden
    assert!(p.steps[2].hidden);
    assert_eq!(p.visible_steps(), 3);
    assert_eq!(p.search_depth(), 2);
    assert_eq!(p.midpoint().unwrap().rid, 4);
    assert_eq!(p.cost, 5.0);
    assert!(p.steps.iter().skip(1).all(|s| s.from_is_parent));
    let p = graph
        .shortest(
            1,
            5,
            Options {
                hidden: &[2, 3, 4],
                ..options()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(p.visible_steps(), 1);
    assert_eq!(p.search_depth(), 0);
    assert!(p.midpoint().is_none());
}

#[test]
/// A search follows a cherrypick only as the first hop, and does not follow an excluded one.
fn first_hop_cherrypicks_only_and_exclusions() {
    let n = small_nodes();
    let picks = [
        Cherrypick {
            parent: 1,
            child: 3,
            excluded: false,
        },
        Cherrypick {
            parent: 1,
            child: 6,
            excluded: true,
        },
        Cherrypick {
            parent: 4,
            child: 5,
            excluded: false,
        },
    ];
    let graph = Graph::new(&n, &[link(3, 4)], &picks, limits()).unwrap();
    let o = Options {
        direction: Direction::ForwardWithCherrypicks,
        ..options()
    };
    assert_eq!(ids(&graph.shortest(1, 4, o).unwrap().unwrap()), [1, 3, 4]);
    assert!(graph.shortest(1, 5, o).unwrap().is_none());
    assert!(graph.shortest(1, 6, o).unwrap().is_none());
    assert!(
        graph
            .shortest(
                1,
                4,
                Options {
                    direction: Direction::Forward,
                    ..o
                }
            )
            .unwrap()
            .is_none()
    );
}

#[test]
/// Branch cost uses time distance instead of arc count.
fn branch_cost_uses_time_distance_instead_of_arc_count() {
    let nodes = [
        Checkin {
            rid: 1,
            branch: b"trunk",
            mtime: 0.0,
        },
        Checkin {
            rid: 2,
            branch: b"trunk",
            mtime: 100.0,
        },
        Checkin {
            rid: 3,
            branch: b"feature",
            mtime: 0.0,
        },
        Checkin {
            rid: 4,
            branch: b"trunk",
            mtime: 1.0,
        },
    ];
    let graph = Graph::new(
        &nodes,
        &[link(1, 2), link(1, 3), link(2, 4), link(3, 4)],
        &[],
        limits(),
    )
    .unwrap();
    let p = graph
        .shortest(
            1,
            4,
            Options {
                branch_cost: 1,
                ..options()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(ids(&p), [1, 3, 4]);
    assert_eq!(p.cost, 3.0);
    let p = graph
        .shortest(
            1,
            4,
            Options {
                branch_cost: 200,
                ..options()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(ids(&p), [1, 2, 4]);
    assert_eq!(p.cost, 199.0);
}

#[test]
/// Bounds reject inputs and search growth.
fn bounds_reject_inputs_and_search_growth() {
    let n = small_nodes();
    for l in [
        Limits {
            checkins: 5,
            ..limits()
        },
        Limits {
            links: 0,
            ..limits()
        },
        Limits {
            branch_bytes: 1,
            ..limits()
        },
    ] {
        assert!(matches!(
            Graph::new(&n, &[link(1, 2)], &[], l),
            Err(Error::Limit)
        ));
    }
    for l in [
        Limits {
            search_nodes: 1,
            ..limits()
        },
        Limits {
            row_visits: 0,
            ..limits()
        },
    ] {
        let graph = Graph::new(&n, &[link(1, 2)], &[], l).unwrap();
        assert!(matches!(graph.shortest(1, 2, options()), Err(Error::Limit)));
        assert!(matches!(graph.common_ancestor(1, 2), Err(Error::Limit)));
    }
    let graph = Graph::new(
        &n,
        &[],
        &[],
        Limits {
            search_nodes: 0,
            ..limits()
        },
    )
    .unwrap();
    assert!(matches!(graph.shortest(1, 1, options()), Err(Error::Limit)));
    assert!(matches!(graph.common_ancestor(1, 1), Err(Error::Limit)));
}

#[test]
/// Malformed graph and options are errors.
fn malformed_graph_and_options_are_errors() {
    let mut n = small_nodes();
    n[1].rid = 1;
    assert!(matches!(
        Graph::new(&n, &[], &[], limits()),
        Err(Error::InvalidGraph)
    ));
    n[1].rid = 0;
    assert!(matches!(
        Graph::new(&n, &[], &[], limits()),
        Err(Error::InvalidGraph)
    ));
    n = small_nodes();
    n[0].mtime = f64::NAN;
    assert!(matches!(
        Graph::new(&n, &[], &[], limits()),
        Err(Error::InvalidGraph)
    ));
    n[0].mtime = f64::INFINITY;
    assert!(matches!(
        Graph::new(&n, &[], &[], limits()),
        Err(Error::InvalidGraph)
    ));
    n = small_nodes();
    n[0].branch = b"trunk\0tail";
    assert!(matches!(
        Graph::new(&n, &[], &[], limits()),
        Err(Error::InvalidGraph)
    ));
    n = small_nodes();
    assert!(matches!(
        Graph::new(&n, &[link(1, 99)], &[], limits()),
        Err(Error::InvalidGraph)
    ));
    assert!(matches!(
        Graph::new(
            &n,
            &[],
            &[Cherrypick {
                parent: 99,
                child: 1,
                excluded: true
            }],
            limits()
        ),
        Err(Error::InvalidGraph)
    ));
    let graph = Graph::new(&n, &[], &[], limits()).unwrap();
    assert!(matches!(
        graph.shortest(0, 1, options()),
        Err(Error::InvalidGraph)
    ));
    assert!(matches!(
        graph.shortest(1, 99, options()),
        Err(Error::InvalidGraph)
    ));
    assert!(matches!(
        graph.shortest(
            1,
            2,
            Options {
                hidden: &[99],
                ..options()
            }
        ),
        Err(Error::InvalidGraph)
    ));
    assert!(matches!(
        graph.shortest(
            1,
            2,
            Options {
                branch_cost: u32::MAX,
                ..options()
            }
        ),
        Err(Error::Limit)
    ));
    // The hidden list may hold at most `limits.checkins` entries, repeated identifiers included.
    let at_bound = vec![2; 100];
    let over_bound = vec![2; 101];
    assert!(
        graph
            .shortest(
                1,
                2,
                Options {
                    hidden: &at_bound,
                    ..options()
                }
            )
            .is_ok()
    );
    assert!(matches!(
        graph.shortest(
            1,
            2,
            Options {
                hidden: &over_bound,
                ..options()
            }
        ),
        Err(Error::Limit)
    ));
    assert_eq!(graph.common_ancestor(0, 1).unwrap(), None);
    assert_eq!(graph.common_ancestor(1, 0).unwrap(), None);
    assert!(matches!(
        graph.common_ancestor(1, 99),
        Err(Error::InvalidGraph)
    ));
}

#[test]
/// A finite time difference that overflows is rejected.
fn overflowing_finite_time_difference_is_rejected() {
    let nodes = [
        Checkin {
            rid: 1,
            branch: b"trunk",
            mtime: f64::MAX,
        },
        Checkin {
            rid: 2,
            branch: b"trunk",
            mtime: -f64::MAX,
        },
    ];
    let graph = Graph::new(&nodes, &[link(1, 2)], &[], limits()).unwrap();
    assert!(matches!(
        graph.shortest(
            1,
            2,
            Options {
                branch_cost: 1,
                ..options()
            }
        ),
        Err(Error::NonFiniteCost)
    ));
    assert!(graph.shortest(1, 2, options()).unwrap().is_some());
}

#[test]
/// Searches over duplicate rows and cycles end under the native rules for seen nodes.
fn duplicate_rows_and_cycles_terminate_under_native_seen_rules() {
    let n = small_nodes();
    let graph = Graph::new(
        &n,
        &[link(1, 2), link(1, 2), link(2, 1), link(2, 3)],
        &[],
        limits(),
    )
    .unwrap();
    assert_eq!(
        ids(&graph.shortest(1, 3, options()).unwrap().unwrap()),
        [1, 2, 3]
    );
    assert!(graph.shortest(1, 6, options()).unwrap().is_none());
    // A repeated hidden identifier is accepted, as the native bag of hidden nodes accepts it.
    let repeated = Options {
        hidden: &[2, 2],
        ..options()
    };
    let p = graph.shortest(1, 3, repeated).unwrap().unwrap();
    assert_eq!(ids(&p), [1, 2, 3]);
    assert!(p.steps[1].hidden);
    assert!(graph.common_ancestor(1, 6).unwrap().is_none());
    assert_eq!(graph.common_ancestor(1, 3).unwrap().unwrap().pivot, 2);
}

/// Native ancestor rewiring may reverse requested endpoints rather than normalize them.
#[test]
fn ancestor_orientation_preserves_the_discovery_side() {
    // Unsorted metadata IDs prove the lookup index does not reorder native edge rows.
    let nodes = [
        Checkin {
            rid: 3,
            branch: b"trunk",
            mtime: 3.0,
        },
        Checkin {
            rid: 1,
            branch: b"trunk",
            mtime: 1.0,
        },
        Checkin {
            rid: 2,
            branch: b"trunk",
            mtime: 2.0,
        },
    ];
    let graph = Graph::new(&nodes, &[link(1, 2), link(1, 3)], &[], limits()).unwrap();
    let ancestor = graph.common_ancestor(2, 3).unwrap().unwrap();
    assert_eq!(ancestor.pivot, 1);
    assert_eq!(
        ancestor.steps.iter().map(|s| s.rid).collect::<Vec<_>>(),
        [3, 1, 2]
    );
    let reverse = graph.common_ancestor(3, 2).unwrap().unwrap();
    assert_eq!(reverse.pivot, 1);
    assert_eq!(
        reverse.steps.iter().map(|s| s.rid).collect::<Vec<_>>(),
        [2, 1, 3]
    );
    assert!(ancestor.steps.iter().all(|s| !s.from_is_parent));
}

/// A malformed graph gives a typed error with a stable public message.
#[test]
fn malformed_graph_refusal_has_public_diagnostic() {
    let mut nodes = small_nodes();
    nodes[1].rid = nodes[0].rid;
    let error = match Graph::new(&nodes, &[], &[], limits()) {
        Err(error) => error,
        Ok(_) => panic!("duplicate native identity must be rejected"),
    };
    assert_eq!(error.to_string(), "history graph error: InvalidGraph");
    assert!(Graph::new(&small_nodes(), &[], &[], limits()).is_ok());
}

/// Pin the cited lines of the Fossil source at check-in `7e8ecf0ce4` in the page and the record.
///
/// The pristine `src/path.c` has `path_shortest` at lines 184 to 271 and `path_common_ancestor`
/// at lines 396 to 447. The pristine `src/pqueue.c` has `pqueuex_rebalance` at lines 99 to 109
/// and `pqueuex_pop` at lines 131 to 146.
#[test]
fn fossil_source_citations() {
    let page = include_str!("../docs/history.md").replace('\n', " ");
    for cited in [
        "| `path_new_node` | `src/path.c` lines 106 to 138 |",
        "| `path_shortest` | `src/path.c` lines 184 to 271 |",
        "| `path_midpoint` | `src/path.c` lines 277 to 285 |",
        "| `path_search_depth` | `src/path.c` lines 313 to 317 |",
        "| `path_common_ancestor` | `src/path.c` lines 396 to 447 |",
        "| `pqueuex_rebalance` | `src/pqueue.c` lines 99 to 109 |",
        "| `pqueuex_pop` | `src/pqueue.c` lines 131 to 146 |",
        "`src/branch.c`, lines 57 to 73",
        "`src/descendants.c`, lines 166 to 175",
        "`src/path.c` lines 226 to 240",
    ] {
        assert!(page.contains(cited), "{cited}");
    }
    let record: Value = serde_json::from_str(include_str!(
        "../evidence/history/source-correspondence.json"
    ))
    .unwrap();
    assert_eq!(
        record["source_pin"],
        "7e8ecf0ce4a3c05aa39de7ffe2828d23e334efe446ef2387e09f205ef00b2227"
    );
    let cited = |file: &str, name: &str| {
        record["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["file"] == file)
            .unwrap()["cited"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == name)
            .unwrap()["lines"]
            .clone()
    };
    assert_eq!(cited("src/path.c", "path_new_node"), "106-138");
    assert_eq!(cited("src/path.c", "path_shortest"), "184-271");
    assert_eq!(cited("src/path.c", "path_midpoint"), "277-285");
    assert_eq!(cited("src/path.c", "path_search_depth"), "313-317");
    assert_eq!(cited("src/path.c", "path_common_ancestor"), "396-447");
    assert_eq!(cited("src/pqueue.c", "pqueuex_rebalance"), "99-109");
    assert_eq!(cited("src/pqueue.c", "pqueuex_pop"), "131-146");
    assert_eq!(cited("src/branch.c", "branch_of_rid"), "57-73");
    assert_eq!(cited("src/descendants.c", "mtime_of_rid"), "166-175");
    let sites = |key: &str| {
        record[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|site| format!("{}:{}", site["file"].as_str().unwrap(), site["line"]))
            .collect::<Vec<_>>()
    };
    assert_eq!(sites("entry_point_call_sites").len(), 17);
    assert_eq!(
        sites("other_call_sites"),
        [
            "src/bisect.c:134",
            "src/bisect.c:136",
            "src/bisect.c:137",
            "src/bisect.c:147",
            "src/bisect.c:151",
            "src/bisect.c:323",
            "src/bisect.c:694",
            "src/bisect.c:697",
            "src/bisect.c:703",
            "src/bisect.c:704",
            "src/merge.c:705",
            "src/timeline.c:2385",
            "src/timeline.c:2411",
            "src/timeline.c:2412",
            "src/timeline.c:2425",
            "src/update.c:300",
        ]
    );
    assert!(page.contains("17 lines for the three search functions and 16 lines in other files"));
}

/// Pin the status fields of the record and the fixtures to what this tree has run.
///
/// The port is compiled and tested, both native captures came from the Fossil 2.29 reference
/// binary, and no persisted history or command uses the module.
#[test]
fn fixture_status_fields() {
    let record: Value = serde_json::from_str(include_str!(
        "../evidence/history/source-correspondence.json"
    ))
    .unwrap();
    assert_eq!(
        record["status"],
        "port implemented, compiled and tested by tests/history.rs and the unit tests in \
         src/history.rs; native diagnostics captured from the Fossil 2.29 reference binary"
    );
    assert_eq!(
        record["scope"],
        "bounded explicit graph algorithm only; no persisted history admission API and no command"
    );
    assert_eq!(record["native_experiment"]["cases"], 25);
    let controls: Vec<_> = record["new_controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    let tests = include_str!("history.rs");
    let units = include_str!("../src/history.rs");
    for name in &controls {
        let file = if name.starts_with("history::tests::") {
            units
        } else {
            tests
        };
        let function = name.trim_start_matches("history::tests::");
        assert!(file.contains(&format!("fn {function}()")), "{name}");
    }
    // Every test except the capture replay is a listed control: 12 here and 3 unit tests.
    assert_eq!(controls.len(), 15);
    assert_eq!(
        record["native_queue_experiment"]["candidate_status"],
        "run by just test"
    );
    assert_eq!(
        record["candidate_runs"]["status"],
        "run by just check on the committed tree"
    );
    let native: Value = serde_json::from_str(include_str!("fixtures/history/native.json")).unwrap();
    let queue: Value =
        serde_json::from_str(include_str!("fixtures/history/native-queue.json")).unwrap();
    assert_eq!(
        native["native_version"],
        "This is fossil version 2.29 [7e8ecf0ce4] 2026-11-16 11:41:54 UTC"
    );
    for capture in [&native, &queue] {
        assert_eq!(
            capture["binary_sha256"],
            "75550c754e94a4b7fa0269f40ce8e4918daf67e05ba12b236ed735d424c8e16c"
        );
        assert_eq!(capture["format"], 1);
    }
    assert_eq!(queue["returncode"], 0);
}
