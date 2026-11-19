//! RFC 057 persistence migration registry.
//!
//! Registry metadata, canonical path selection, and edge-specific
//! backend record transforms for RFC 057 persistence migrations.
//! Staging, validation, and swap mechanics live in the maintenance
//! command, but they must use this compiled registry rather than
//! maintaining a parallel list.

use kombat_chain_state_db::{current_persistence_fingerprint, Namespace};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub(crate) mod legacy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersistenceMigration {
    pub id: &'static str,
    pub source_fingerprint: [u8; 32],
    pub target_fingerprint: [u8; 32],
    pub description: &'static str,
    pub changed_records: &'static [&'static str],
    pub backend_strategy: BackendMigrationStrategy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMigrationStrategy {
    /// Copy every backend namespace byte-for-byte, except for the RFC
    /// 055 fingerprint marker written by the target backend commit.
    /// This is valid only for representation-compatible edges.
    CopyAllNamespaces,
    /// The registry can advertise the path, but backend execution must
    /// not proceed until an edge-specific transform lands.
    NotImplemented,
    /// RFC 056 split `PolicyRecord.account` into
    /// `authority_account + execution_scope`. The canonical engine
    /// record is decoded with the legacy policy layout and re-encoded
    /// with direct-account execution scopes.
    Rfc056PolicyAuthorityScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationRegistryError {
    DuplicateEdgeId(&'static str),
    NonProductionStrategy {
        id: &'static str,
        strategy: BackendMigrationStrategy,
    },
    DuplicateEdgeTriple {
        source_fingerprint: [u8; 32],
        target_fingerprint: [u8; 32],
        id: &'static str,
    },
    DuplicateEdgePair {
        source_fingerprint: [u8; 32],
        target_fingerprint: [u8; 32],
        existing_id: &'static str,
        duplicate_id: &'static str,
    },
    CycleDetected {
        source_fingerprint: [u8; 32],
    },
}

impl fmt::Display for MigrationRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigrationRegistryError::DuplicateEdgeId(id) => {
                write!(f, "duplicate migration edge id {id}")
            }
            MigrationRegistryError::NonProductionStrategy { id, strategy } => write!(
                f,
                "migration edge {id} uses non-production-ready backend strategy {strategy:?}"
            ),
            MigrationRegistryError::DuplicateEdgeTriple {
                source_fingerprint,
                target_fingerprint,
                id,
            } => write!(
                f,
                "duplicate migration edge triple id={id} source=0x{} target=0x{}",
                hex_bytes(source_fingerprint),
                hex_bytes(target_fingerprint)
            ),
            MigrationRegistryError::DuplicateEdgePair {
                source_fingerprint,
                target_fingerprint,
                existing_id,
                duplicate_id,
            } => write!(
                f,
                "ambiguous migration edges source=0x{} target=0x{} ids={existing_id},{duplicate_id}",
                hex_bytes(source_fingerprint),
                hex_bytes(target_fingerprint)
            ),
            MigrationRegistryError::CycleDetected { source_fingerprint } => write!(
                f,
                "migration registry cycle detected from 0x{}",
                hex_bytes(source_fingerprint)
            ),
        }
    }
}

impl std::error::Error for MigrationRegistryError {}

pub fn current_binary_persistence_fingerprint() -> [u8; 32] {
    current_persistence_fingerprint()
}

pub fn compiled_persistence_migrations() -> &'static [PersistenceMigration] {
    // No production migration edge is registered yet. The shared
    // testnet is still resettable, and RFC 056 was deployed through
    // that reset-only path. Non-resettable environments must replace
    // this empty registry with explicit retained edges before accepting
    // fingerprint-mismatched state.
    &[]
}

pub fn validate_compiled_persistence_migrations(
    migrations: &[PersistenceMigration],
) -> Result<(), MigrationRegistryError> {
    validate_migration_registry(migrations)?;
    for migration in migrations {
        if !backend_migration_strategy_is_production_ready(migration.backend_strategy) {
            return Err(MigrationRegistryError::NonProductionStrategy {
                id: migration.id,
                strategy: migration.backend_strategy,
            });
        }
    }
    Ok(())
}

pub fn backend_migration_strategy_is_production_ready(strategy: BackendMigrationStrategy) -> bool {
    match strategy {
        BackendMigrationStrategy::CopyAllNamespaces => true,
        BackendMigrationStrategy::NotImplemented => false,
        // This edge-specific strategy is retained as a fixture-backed
        // regression harness. It still relies on live aliases for
        // unchanged nested records, so it must not be registered in the
        // compiled production registry until a fingerprint-local legacy
        // module replaces those aliases.
        BackendMigrationStrategy::Rfc056PolicyAuthorityScope => false,
    }
}

pub fn migrate_backend_record(
    migration: &PersistenceMigration,
    namespace: Namespace,
    key: &[u8],
    value: &[u8],
) -> Result<Vec<u8>, String> {
    match migration.backend_strategy {
        BackendMigrationStrategy::CopyAllNamespaces => Ok(value.to_vec()),
        BackendMigrationStrategy::NotImplemented => Err(format!(
            "migration edge {} has no backend execution implementation",
            migration.id
        )),
        BackendMigrationStrategy::Rfc056PolicyAuthorityScope => {
            legacy::rfc056_policy_authority_scope::migrate_rfc056_policy_authority_scope_record(
                namespace, key, value,
            )
        }
    }
}

pub fn value_migrated_namespaces(migration: &PersistenceMigration) -> &'static [Namespace] {
    match migration.backend_strategy {
        BackendMigrationStrategy::CopyAllNamespaces => &[],
        BackendMigrationStrategy::NotImplemented => &[],
        BackendMigrationStrategy::Rfc056PolicyAuthorityScope => &[Namespace::State],
    }
}

pub fn validate_migration_registry(
    migrations: &[PersistenceMigration],
) -> Result<(), MigrationRegistryError> {
    let mut ids = BTreeSet::new();
    let mut triples = BTreeSet::new();
    let mut pairs = BTreeMap::new();
    for migration in migrations {
        let triple = (
            migration.source_fingerprint,
            migration.target_fingerprint,
            migration.id,
        );
        if !triples.insert(triple) {
            return Err(MigrationRegistryError::DuplicateEdgeTriple {
                source_fingerprint: migration.source_fingerprint,
                target_fingerprint: migration.target_fingerprint,
                id: migration.id,
            });
        }
        let pair = (migration.source_fingerprint, migration.target_fingerprint);
        if let Some(existing_id) = pairs.insert(pair, migration.id) {
            return Err(MigrationRegistryError::DuplicateEdgePair {
                source_fingerprint: migration.source_fingerprint,
                target_fingerprint: migration.target_fingerprint,
                existing_id,
                duplicate_id: migration.id,
            });
        }
        if !ids.insert(migration.id) {
            return Err(MigrationRegistryError::DuplicateEdgeId(migration.id));
        }
    }
    reject_registry_cycles(migrations)
}

pub fn canonical_migration_path<'a>(
    migrations: &'a [PersistenceMigration],
    source_fingerprint: [u8; 32],
    target_fingerprint: [u8; 32],
) -> Result<Option<Vec<&'a PersistenceMigration>>, MigrationRegistryError> {
    validate_migration_registry(migrations)?;
    if source_fingerprint == target_fingerprint {
        return Ok(Some(Vec::new()));
    }

    let mut adjacency: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();
    for (index, migration) in migrations.iter().enumerate() {
        adjacency
            .entry(migration.source_fingerprint)
            .or_default()
            .push(index);
    }
    for indexes in adjacency.values_mut() {
        indexes.sort_by(|left, right| {
            migrations[*left]
                .id
                .cmp(migrations[*right].id)
                .then_with(|| {
                    migrations[*left]
                        .target_fingerprint
                        .cmp(&migrations[*right].target_fingerprint)
                })
        });
    }

    let mut visited = BTreeSet::new();
    visited.insert(source_fingerprint);
    let mut current_path = Vec::new();
    let mut best_path: Option<Vec<usize>> = None;
    search_paths(
        migrations,
        &adjacency,
        source_fingerprint,
        target_fingerprint,
        &mut visited,
        &mut current_path,
        &mut best_path,
    );

    Ok(best_path.map(|path| path.into_iter().map(|index| &migrations[index]).collect()))
}

fn reject_registry_cycles(
    migrations: &[PersistenceMigration],
) -> Result<(), MigrationRegistryError> {
    let mut adjacency: BTreeMap<[u8; 32], Vec<[u8; 32]>> = BTreeMap::new();
    for migration in migrations {
        adjacency
            .entry(migration.source_fingerprint)
            .or_default()
            .push(migration.target_fingerprint);
        adjacency.entry(migration.target_fingerprint).or_default();
    }

    let mut states: BTreeMap<[u8; 32], VisitState> = BTreeMap::new();
    for node in adjacency.keys().copied().collect::<Vec<_>>() {
        if !matches!(states.get(&node), Some(VisitState::Done)) {
            visit_for_cycle(node, &adjacency, &mut states)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

fn visit_for_cycle(
    node: [u8; 32],
    adjacency: &BTreeMap<[u8; 32], Vec<[u8; 32]>>,
    states: &mut BTreeMap<[u8; 32], VisitState>,
) -> Result<(), MigrationRegistryError> {
    if matches!(states.get(&node), Some(VisitState::Visiting)) {
        return Err(MigrationRegistryError::CycleDetected {
            source_fingerprint: node,
        });
    }
    if matches!(states.get(&node), Some(VisitState::Done)) {
        return Ok(());
    }

    states.insert(node, VisitState::Visiting);
    for next in adjacency.get(&node).into_iter().flatten().copied() {
        visit_for_cycle(next, adjacency, states)?;
    }
    states.insert(node, VisitState::Done);
    Ok(())
}

fn search_paths(
    migrations: &[PersistenceMigration],
    adjacency: &BTreeMap<[u8; 32], Vec<usize>>,
    current: [u8; 32],
    target: [u8; 32],
    visited: &mut BTreeSet<[u8; 32]>,
    current_path: &mut Vec<usize>,
    best_path: &mut Option<Vec<usize>>,
) {
    if let Some(best) = best_path.as_ref() {
        if current_path.len() >= best.len() {
            return;
        }
    }

    let Some(edges) = adjacency.get(&current) else {
        return;
    };

    for edge_index in edges {
        let edge = &migrations[*edge_index];
        if !visited.insert(edge.target_fingerprint) {
            continue;
        }
        current_path.push(*edge_index);
        if edge.target_fingerprint == target {
            if best_path
                .as_ref()
                .is_none_or(|best| path_is_canonical(migrations, current_path, best))
            {
                *best_path = Some(current_path.clone());
            }
        } else {
            search_paths(
                migrations,
                adjacency,
                edge.target_fingerprint,
                target,
                visited,
                current_path,
                best_path,
            );
        }
        current_path.pop();
        visited.remove(&edge.target_fingerprint);
    }
}

fn path_is_canonical(
    migrations: &[PersistenceMigration],
    candidate: &[usize],
    current_best: &[usize],
) -> bool {
    if candidate.len() != current_best.len() {
        return candidate.len() < current_best.len();
    }
    candidate
        .iter()
        .map(|index| migrations[*index].id)
        .cmp(current_best.iter().map(|index| migrations[*index].id))
        .is_lt()
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: [u8; 32] = [0x0a; 32];
    const B: [u8; 32] = [0x0b; 32];
    const C: [u8; 32] = [0x0c; 32];
    const D: [u8; 32] = [0x0d; 32];

    fn edge(
        id: &'static str,
        source_fingerprint: [u8; 32],
        target_fingerprint: [u8; 32],
    ) -> PersistenceMigration {
        PersistenceMigration {
            id,
            source_fingerprint,
            target_fingerprint,
            description: "test edge",
            changed_records: &["test"],
            backend_strategy: BackendMigrationStrategy::CopyAllNamespaces,
        }
    }

    #[test]
    fn compiled_registry_is_valid() {
        validate_compiled_persistence_migrations(compiled_persistence_migrations()).unwrap();
    }

    #[test]
    fn compiled_registry_rejects_non_production_strategies() {
        let migrations = [PersistenceMigration {
            id: "rfc056-fixture-only",
            source_fingerprint: A,
            target_fingerprint: B,
            description: "fixture-only edge",
            changed_records: &["engine_canonical_v1"],
            backend_strategy: BackendMigrationStrategy::Rfc056PolicyAuthorityScope,
        }];
        let err = validate_compiled_persistence_migrations(&migrations).unwrap_err();
        assert!(matches!(
            err,
            MigrationRegistryError::NonProductionStrategy {
                id: "rfc056-fixture-only",
                strategy: BackendMigrationStrategy::Rfc056PolicyAuthorityScope
            }
        ));
    }

    #[test]
    fn canonical_path_returns_none_for_unknown_source() {
        let migrations = [edge("a-to-b", A, B)];
        let path = canonical_migration_path(&migrations, C, B).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn canonical_path_returns_direct_edge() {
        let migrations = [edge("a-to-b", A, B)];
        let path = canonical_migration_path(&migrations, A, B)
            .unwrap()
            .unwrap();
        assert_eq!(
            path.iter().map(|edge| edge.id).collect::<Vec<_>>(),
            ["a-to-b"]
        );
    }

    #[test]
    fn canonical_path_prefers_shortest_path() {
        let migrations = [
            edge("a-to-b", A, B),
            edge("b-to-c", B, C),
            edge("a-to-c", A, C),
        ];
        let path = canonical_migration_path(&migrations, A, C)
            .unwrap()
            .unwrap();
        assert_eq!(
            path.iter().map(|edge| edge.id).collect::<Vec<_>>(),
            ["a-to-c"]
        );
    }

    #[test]
    fn canonical_path_tiebreaks_on_ordered_edge_ids() {
        let migrations = [
            edge("b-path-1", A, B),
            edge("b-path-2", B, D),
            edge("a-path-1", A, C),
            edge("z-path-2", C, D),
        ];
        let path = canonical_migration_path(&migrations, A, D)
            .unwrap()
            .unwrap();
        assert_eq!(
            path.iter().map(|edge| edge.id).collect::<Vec<_>>(),
            ["a-path-1", "z-path-2"]
        );
    }

    #[test]
    fn registry_rejects_duplicate_ids() {
        let migrations = [edge("same", A, B), edge("same", B, C)];
        let err = validate_migration_registry(&migrations).unwrap_err();
        assert!(matches!(
            err,
            MigrationRegistryError::DuplicateEdgeId("same")
        ));
    }

    #[test]
    fn registry_rejects_duplicate_triples() {
        let migrations = [edge("same", A, B), edge("same", A, B)];
        let err = validate_migration_registry(&migrations).unwrap_err();
        assert!(matches!(
            err,
            MigrationRegistryError::DuplicateEdgeTriple { id: "same", .. }
        ));
    }

    #[test]
    fn registry_rejects_ambiguous_duplicate_source_target_pairs() {
        let migrations = [edge("a-to-b-one", A, B), edge("a-to-b-two", A, B)];
        let err = validate_migration_registry(&migrations).unwrap_err();
        assert!(matches!(
            err,
            MigrationRegistryError::DuplicateEdgePair {
                existing_id: "a-to-b-one",
                duplicate_id: "a-to-b-two",
                ..
            }
        ));
    }

    #[test]
    fn registry_rejects_cycles() {
        let migrations = [
            edge("a-to-b", A, B),
            edge("b-to-c", B, C),
            edge("c-to-a", C, A),
        ];
        let err = validate_migration_registry(&migrations).unwrap_err();
        assert!(matches!(err, MigrationRegistryError::CycleDetected { .. }));
    }
}
