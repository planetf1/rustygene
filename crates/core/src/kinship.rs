//! Kinship relationship calculator.
//!
//! Given a shortest path between two persons in the family tree, compute the
//! canonical kinship name (e.g., "2nd cousin once removed", "uncle", "sibling").

use crate::types::EntityId;

/// Result of kinship calculation between two persons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KinshipResult {
    /// Canonical kinship name (e.g., "parent", "2nd cousin once removed")
    pub kinship_name: String,
    /// The direction labels along the path (e.g., ["parent", "sibling"])
    pub path_labels: Vec<String>,
    /// The most recent common ancestor, if found
    pub common_ancestor_id: Option<EntityId>,
}

/// A step in a shortest path between two persons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStep {
    /// Direction of relationship (e.g., "up", "down", "sibling")
    pub direction: String,
    /// Label of relationship (e.g., "parent", "child", "spouse")
    pub label: String,
    /// The person at this step
    pub person_id: EntityId,
}

/// Compute kinship name from a path of relationship steps.
///
/// # Algorithm
///
/// 1. Parse the path to find the turning point (where we stop going "up" and start going "down")
/// 2. Count generations up from person A to MRCA
/// 3. Count generations down from MRCA to person B
/// 4. Apply kinship formula to produce canonical names
///
/// # Examples
///
/// - `["parent"]` → "parent"
/// - `["child"]` → "child"
/// - `["parent", "parent", "child"]` → "grandparent"
/// - `["parent", "sibling"]` → "sibling" (assuming same generation)
/// - Multiple steps up then down → cousin names
pub fn compute_kinship(path: &[PathStep]) -> KinshipResult {
    let labels: Vec<_> = path.iter().map(|s| s.label.clone()).collect();

    // Edge case: self reference
    if path.is_empty() || (path.len() == 1 && path[0].label == "self") {
        return KinshipResult {
            kinship_name: "self".to_string(),
            path_labels: labels,
            common_ancestor_id: path.first().map(|s| s.person_id),
        };
    }

    // Trace upward to find MRCA, then downward to the target
    let (up_count, down_count, mrca_id) = trace_generations(path);

    let kinship_name = if up_count == 0 && down_count == 1 {
        "child".to_string()
    } else if up_count == 1 && down_count == 0 {
        "parent".to_string()
    } else if up_count == 0 && down_count == 2 {
        "grandchild".to_string()
    } else if up_count == 2 && down_count == 0 {
        "grandparent".to_string()
    } else if up_count == 0 && down_count >= 3 {
        let greats = "great-".repeat(down_count.saturating_sub(2));
        format!("{}grandchild", greats)
    } else if up_count >= 3 && down_count == 0 {
        let greats = "great-".repeat(up_count.saturating_sub(2));
        format!("{}grandparent", greats)
    } else if up_count == 1 && down_count == 1 {
        "sibling".to_string()
    } else if up_count == 2 && down_count == 1 {
        "uncle".to_string()
    } else if up_count == 2 && down_count == 2 {
        "cousin".to_string()
    } else if up_count == 1 && down_count == 2 {
        "niece_or_nephew".to_string()
    } else if up_count == 0 && down_count == 0 {
        "same_person".to_string()
    } else {
        // General cousin formula: cousin degree = min(up, down) - 1, removed = |up - down|
        let cousin_degree = (up_count.min(down_count)).saturating_sub(1);
        let times_removed = (up_count.abs_diff(down_count)).max(0);

        if cousin_degree == 0 {
            if times_removed == 1 {
                if up_count < down_count {
                    "niece_or_nephew".to_string()
                } else {
                    "aunt_or_uncle".to_string()
                }
            } else if times_removed > 1 {
                let greats = "great-".repeat(times_removed.saturating_sub(1));
                if up_count < down_count {
                    format!("{}niece_or_nephew", greats)
                } else {
                    format!("{}aunt_or_uncle", greats)
                }
            } else {
                "relative".to_string()
            }
        } else {
            let degree_name = match cousin_degree {
                0 => "1st cousin".to_string(),
                1 => "2nd cousin".to_string(),
                2 => "3rd cousin".to_string(),
                n => format!("{}th cousin", n + 1),
            };

            if times_removed == 0 {
                degree_name
            } else {
                format!("{} {} removed", degree_name, times_removed)
            }
        }
    };

    KinshipResult {
        kinship_name,
        path_labels: labels,
        common_ancestor_id: mrca_id,
    }
}

/// Trace upward and downward generations from a path.
///
/// Returns (up_count, down_count, mrca_id).
fn trace_generations(path: &[PathStep]) -> (usize, usize, Option<EntityId>) {
    let mut up_count = 0;
    let mut down_count = 0;
    let mut mrca_id: Option<EntityId> = None;
    let mut phase = Phase::Up; // Start by going up

    for step in path {
        match &step.direction[..] {
            "up" => {
                if phase == Phase::Down {
                    // Unexpected: already going down, now going up again (shouldn't happen in shortest path)
                    phase = Phase::Up;
                    down_count = 0;
                }
                up_count += 1;
                mrca_id = Some(step.person_id);
            }
            "down" => {
                if phase == Phase::Up {
                    phase = Phase::Down;
                }
                down_count += 1;
            }
            "sibling" => {
                // Sibling is lateral movement; doesn't count as up or down
                if phase == Phase::Up {
                    up_count += 1; // Going to sibling is like going to parent then to sibling child
                    mrca_id = Some(step.person_id);
                    phase = Phase::Down;
                } else {
                    down_count += 1;
                }
            }
            "spouse" => {
                // Spouse is lateral; doesn't affect generation count
            }
            "none" | "self" => {
                // Self-reference, no generation change
            }
            _ => {
                // Unknown direction; treat as neutral
            }
        }
    }

    (up_count, down_count, mrca_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn id(s: &str) -> EntityId {
        EntityId(Uuid::parse_str(s).unwrap())
    }

    #[test]
    fn parent_child_relationships() {
        let path = vec![PathStep {
            direction: "up".to_string(),
            label: "parent".to_string(),
            person_id: id("00000000-0000-0000-0000-000000000001"),
        }];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "parent");
    }

    #[test]
    fn child_relationship() {
        let path = vec![PathStep {
            direction: "down".to_string(),
            label: "child".to_string(),
            person_id: id("00000000-0000-0000-0000-000000000001"),
        }];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "child");
    }

    #[test]
    fn sibling_relationship() {
        let parent_id = id("00000000-0000-0000-0000-000000000001");
        let path = vec![
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: parent_id,
            },
            PathStep {
                direction: "down".to_string(),
                label: "child".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000002"),
            },
        ];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "sibling");
    }

    #[test]
    fn grandparent_relationship() {
        let path = vec![
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000001"),
            },
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000002"),
            },
        ];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "grandparent");
    }

    #[test]
    fn uncle_or_aunt_relationship() {
        // Path from person to their uncle: up 2 (to grandparent), down 1 (to uncle)
        let grandparent_id = id("00000000-0000-0000-0000-000000000002");
        let path = vec![
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000001"),
            },
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: grandparent_id,
            },
            PathStep {
                direction: "down".to_string(),
                label: "child".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000003"),
            },
        ];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "uncle");
    }

    #[test]
    fn cousin_relationship() {
        let path = vec![
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000001"),
            },
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000002"),
            },
            PathStep {
                direction: "down".to_string(),
                label: "child".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000003"),
            },
            PathStep {
                direction: "down".to_string(),
                label: "child".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000004"),
            },
        ];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "cousin");
    }

    #[test]
    fn second_cousin_once_removed() {
        let path = vec![
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000001"),
            },
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000002"),
            },
            PathStep {
                direction: "up".to_string(),
                label: "parent".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000003"),
            },
            PathStep {
                direction: "down".to_string(),
                label: "child".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000004"),
            },
            PathStep {
                direction: "down".to_string(),
                label: "child".to_string(),
                person_id: id("00000000-0000-0000-0000-000000000005"),
            },
        ];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "2nd cousin 1 removed");
    }

    #[test]
    fn self_reference() {
        let path = vec![PathStep {
            direction: "none".to_string(),
            label: "self".to_string(),
            person_id: id("00000000-0000-0000-0000-000000000001"),
        }];

        let result = compute_kinship(&path);
        assert_eq!(result.kinship_name, "self");
    }
}
