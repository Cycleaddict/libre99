#!/usr/bin/env python3
# Observatory persistent game atlas. Licensed under LICENSE.md (Modified MIT
# with Commons Clause). Standard library only; it reads tracked JSON atlas
# packages, it does not run the emulator or ingest traces.

"""Append-only observatory atlas: validate packages, answer bounded queries.

The atlas holds accepted reconstruction evidence in ordinary JSON packages so a
fresh task can recover a causal chain and its evidence labels without rereading
raw traces. It stores no commercial bytes, media paths, traces, or owner paths.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

ATLAS_VERSION = 1

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ROOT = REPO_ROOT / "observatory" / "atlas"

# Evidence labels. Facts carry any of them; semantic hypotheses carry only the
# two that admit no direct observation.
CLASSIFICATIONS: Tuple[str, ...] = (
    "observed",
    "source-confirmed",
    "corroborated",
    "inferred",
    "unresolved",
)
SEMANTIC_CLASSIFICATIONS: Tuple[str, ...] = ("inferred", "unresolved")

ENTITY_KINDS: Tuple[str, ...] = (
    "subsystem",
    "experiment",
    "input",
    "routine",
    "operation",
    "state-cell",
    "effect",
)
EVIDENCE_KINDS: Tuple[str, ...] = (
    "primary-documentation",
    "tracked-document",
    "owner-local-experiment",
    "held-out-source",
)
RELATIONSHIP_KINDS: Tuple[str, ...] = (
    "part-of",
    "executed-by",
    "reads",
    "compares",
    "continues-to",
    "branches-to",
    "writes",
    "precedes",
    "causes",
)

ENTITY_RANK = {kind: index for index, kind in enumerate(ENTITY_KINDS)}

FAILURE_EXIT = 1
USAGE_EXIT = 2


class AtlasError(ValueError):
    """Malformed or inconsistent atlas."""


class QueryNotFound(AtlasError):
    """No query matches the requested identity."""


@dataclass(frozen=True)
class Entity:
    id: str
    kind: str
    name: str
    subsystem: str
    order: int
    address: Optional[str]
    space: Optional[str]
    detail: Optional[str]
    package: str


@dataclass(frozen=True)
class Evidence:
    id: str
    kind: str
    ref: str
    detail: Optional[str]
    counts: Mapping[str, Any]
    package: str


@dataclass(frozen=True)
class Fact:
    id: str
    subject: str
    predicate: str
    value: str
    context: Optional[str]
    classification: str
    evidence: Tuple[str, ...]
    note: Optional[str]
    package: str


@dataclass(frozen=True)
class Hypothesis:
    id: str
    subject: str
    statement: str
    classification: str
    alternatives: Tuple[str, ...]
    resolves_when: Optional[str]
    evidence: Tuple[str, ...]
    package: str


@dataclass(frozen=True)
class Relationship:
    id: str
    order: int
    source: str
    kind: str
    target: str
    classification: str
    context: Optional[str]
    detail: Optional[str]
    evidence: Tuple[str, ...]
    package: str


@dataclass(frozen=True)
class Query:
    id: str
    subsystem: str
    question: str
    answer: str
    package: str


@dataclass
class Atlas:
    root: Path
    packages: List[Tuple[str, str]] = field(default_factory=list)
    entities: Dict[str, Entity] = field(default_factory=dict)
    evidence: Dict[str, Evidence] = field(default_factory=dict)
    facts: List[Fact] = field(default_factory=list)
    hypotheses: List[Hypothesis] = field(default_factory=list)
    relationships: List[Relationship] = field(default_factory=list)
    queries: Dict[str, Query] = field(default_factory=dict)
    # id -> (section, package, file name), for the duplicate-id refusal.
    owners: Dict[str, Tuple[str, str, str]] = field(default_factory=dict)


# --- parsing ---------------------------------------------------------------


def _require(obj: Any, where: str) -> Mapping[str, Any]:
    if not isinstance(obj, dict):
        raise AtlasError(f"{where}: expected a JSON object")
    return obj


def _text(obj: Mapping[str, Any], key: str, where: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str) or not value.strip():
        raise AtlasError(f"{where}: missing or empty string field '{key}'")
    return value


def _opt_text(obj: Mapping[str, Any], key: str, where: str) -> Optional[str]:
    value = obj.get(key)
    if value is None:
        return None
    if not isinstance(value, str) or not value.strip():
        raise AtlasError(f"{where}: field '{key}' must be a non-empty string")
    return value


def _choice(obj: Mapping[str, Any], key: str, allowed: Sequence[str], where: str) -> str:
    value = _text(obj, key, where)
    if value not in allowed:
        raise AtlasError(
            f"{where}: unsupported {key} {value!r}; allowed: {', '.join(allowed)}"
        )
    return value


def _order(obj: Mapping[str, Any], where: str) -> int:
    value = obj.get("order", 0)
    if isinstance(value, bool) or not isinstance(value, int):
        raise AtlasError(f"{where}: field 'order' must be an integer")
    return value


def _refs(obj: Mapping[str, Any], key: str, where: str) -> Tuple[str, ...]:
    value = obj.get(key, [])
    if not isinstance(value, list) or not all(
        isinstance(item, str) and item.strip() for item in value
    ):
        raise AtlasError(f"{where}: field '{key}' must be a list of non-empty strings")
    return tuple(value)


def _section(data: Mapping[str, Any], key: str, where: str) -> List[Mapping[str, Any]]:
    value = data.get(key, [])
    if not isinstance(value, list):
        raise AtlasError(f"{where}: section '{key}' must be a list")
    return [_require(item, f"{where}:{key}[{index}]") for index, item in enumerate(value)]


def parse_package(path: Path, atlas: Atlas) -> None:
    """Read one package file into `atlas`, raising on structural problems."""

    where = path.name
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise AtlasError(f"{where}: invalid JSON: {exc}") from exc
    data = _require(data, where)

    version = data.get("atlas_version")
    if version != ATLAS_VERSION:
        raise AtlasError(
            f"{where}: atlas_version must be {ATLAS_VERSION}, found {version!r}"
        )
    package = _text(data, "package", where)
    atlas.packages.append((path.name, package))

    for item in _section(data, "entities", where):
        eid = _text(item, "id", f"{where}:entities")
        spot = f"{where}:entities[{eid}]"
        kind = _choice(item, "kind", ENTITY_KINDS, spot)
        subsystem = _opt_text(item, "subsystem", spot)
        if kind == "subsystem":
            if subsystem is not None and subsystem != eid:
                raise AtlasError(
                    f"{spot}: a subsystem entity may only declare itself, not {subsystem!r}"
                )
            subsystem = eid
        elif subsystem is None:
            raise AtlasError(f"{spot}: missing or empty string field 'subsystem'")
        atlas.entities.setdefault(
            eid,
            Entity(
                id=eid,
                kind=kind,
                name=_text(item, "name", spot),
                subsystem=subsystem,
                order=_order(item, spot),
                address=_opt_text(item, "address", spot),
                space=_opt_text(item, "space", spot),
                detail=_opt_text(item, "detail", spot),
                package=package,
            ),
        )
        _note_id(atlas, eid, "entity", package, path.name)

    for item in _section(data, "evidence", where):
        vid = _text(item, "id", f"{where}:evidence")
        spot = f"{where}:evidence[{vid}]"
        counts = item.get("counts", {})
        if not isinstance(counts, dict):
            raise AtlasError(f"{spot}: field 'counts' must be an object")
        atlas.evidence.setdefault(
            vid,
            Evidence(
                id=vid,
                kind=_choice(item, "kind", EVIDENCE_KINDS, spot),
                ref=_text(item, "ref", spot),
                detail=_opt_text(item, "detail", spot),
                counts=dict(counts),
                package=package,
            ),
        )
        _note_id(atlas, vid, "evidence", package, path.name)

    for item in _section(data, "facts", where):
        fid = _text(item, "id", f"{where}:facts")
        spot = f"{where}:facts[{fid}]"
        atlas.facts.append(
            Fact(
                id=fid,
                subject=_text(item, "subject", spot),
                predicate=_text(item, "predicate", spot),
                value=_text(item, "value", spot),
                context=_opt_text(item, "context", spot),
                classification=_choice(item, "classification", CLASSIFICATIONS, spot),
                evidence=_refs(item, "evidence", spot),
                note=_opt_text(item, "note", spot),
                package=package,
            )
        )
        _note_id(atlas, fid, "fact", package, path.name)

    for item in _section(data, "hypotheses", where):
        hid = _text(item, "id", f"{where}:hypotheses")
        spot = f"{where}:hypotheses[{hid}]"
        atlas.hypotheses.append(
            Hypothesis(
                id=hid,
                subject=_text(item, "subject", spot),
                statement=_text(item, "statement", spot),
                classification=_choice(
                    item, "classification", SEMANTIC_CLASSIFICATIONS, spot
                ),
                alternatives=_refs(item, "alternatives", spot),
                resolves_when=_opt_text(item, "resolves_when", spot),
                evidence=_refs(item, "evidence", spot),
                package=package,
            )
        )
        _note_id(atlas, hid, "hypothesis", package, path.name)

    for item in _section(data, "relationships", where):
        rid = _text(item, "id", f"{where}:relationships")
        spot = f"{where}:relationships[{rid}]"
        atlas.relationships.append(
            Relationship(
                id=rid,
                order=_order(item, spot),
                source=_text(item, "from", spot),
                kind=_choice(item, "kind", RELATIONSHIP_KINDS, spot),
                target=_text(item, "to", spot),
                classification=_choice(item, "classification", CLASSIFICATIONS, spot),
                context=_opt_text(item, "context", spot),
                detail=_opt_text(item, "detail", spot),
                evidence=_refs(item, "evidence", spot),
                package=package,
            )
        )
        _note_id(atlas, rid, "relationship", package, path.name)

    for item in _section(data, "queries", where):
        qid = _text(item, "id", f"{where}:queries")
        spot = f"{where}:queries[{qid}]"
        atlas.queries.setdefault(
            qid,
            Query(
                id=qid,
                subsystem=_text(item, "subsystem", spot),
                question=_text(item, "question", spot),
                answer=_text(item, "answer", spot),
                package=package,
            ),
        )
        _note_id(atlas, qid, "query", package, path.name)


def _note_id(atlas: Atlas, ident: str, section: str, package: str, file_name: str) -> None:
    prior = atlas.owners.get(ident)
    if prior is not None:
        prior_section, prior_package, prior_file = prior
        raise AtlasError(
            f"duplicate id {ident!r}: {prior_section} in {prior_file} "
            f"(package {prior_package}) and {section} in {file_name} (package {package})"
        )
    atlas.owners[ident] = (section, package, file_name)


def load_atlas(root: Path) -> Atlas:
    if not root.is_dir():
        raise AtlasError(f"atlas root not found: {root}")
    files = sorted(p for p in root.glob("*.json") if p.is_file())
    if not files:
        raise AtlasError(f"atlas root holds no *.json packages: {root}")
    atlas = Atlas(root=root)
    for path in files:
        parse_package(path, atlas)
    return atlas


# --- validation ------------------------------------------------------------


def link_problems(atlas: Atlas) -> List[str]:
    """Cross-reference and consistency problems, each naming its offender."""

    problems: List[str] = []

    def check_entity(ref: str, owner: str, field_name: str) -> None:
        if ref not in atlas.entities:
            problems.append(
                f"{owner}: field '{field_name}' references unknown entity {ref!r}"
            )

    def check_evidence(refs: Sequence[str], owner: str) -> None:
        for ref in refs:
            if ref not in atlas.evidence:
                problems.append(f"{owner}: references unknown evidence {ref!r}")

    def check_context(ref: Optional[str], owner: str) -> None:
        if ref is None:
            return
        entity = atlas.entities.get(ref)
        if entity is None:
            problems.append(f"{owner}: field 'context' references unknown entity {ref!r}")
        elif entity.kind != "experiment":
            problems.append(
                f"{owner}: field 'context' references {ref!r}, which is "
                f"{entity.kind}, not an experiment"
            )

    for entity in atlas.entities.values():
        if entity.kind != "subsystem":
            target = atlas.entities.get(entity.subsystem)
            if target is None:
                problems.append(
                    f"entity {entity.id!r}: field 'subsystem' references unknown "
                    f"entity {entity.subsystem!r}"
                )
            elif target.kind != "subsystem":
                problems.append(
                    f"entity {entity.id!r}: field 'subsystem' references {entity.subsystem!r}, "
                    f"which is a {target.kind}, not a subsystem"
                )

    for fact in atlas.facts:
        check_entity(fact.subject, f"fact {fact.id!r}", "subject")
        check_evidence(fact.evidence, f"fact {fact.id!r}")
        check_context(fact.context, f"fact {fact.id!r}")
        if not fact.evidence:
            problems.append(f"fact {fact.id!r}: no evidence reference")

    for hypothesis in atlas.hypotheses:
        check_entity(hypothesis.subject, f"hypothesis {hypothesis.id!r}", "subject")
        check_evidence(hypothesis.evidence, f"hypothesis {hypothesis.id!r}")

    for relationship in atlas.relationships:
        owner = f"relationship {relationship.id!r}"
        check_entity(relationship.source, owner, "from")
        check_entity(relationship.target, owner, "to")
        check_evidence(relationship.evidence, owner)
        check_context(relationship.context, owner)

    for query in atlas.queries.values():
        target = atlas.entities.get(query.subsystem)
        if target is None:
            problems.append(
                f"query {query.id!r}: field 'subsystem' references unknown "
                f"entity {query.subsystem!r}"
            )
        elif target.kind != "subsystem":
            problems.append(
                f"query {query.id!r}: field 'subsystem' references {query.subsystem!r}, "
                f"which is a {target.kind}, not a subsystem"
            )

    problems.extend(contradiction_problems(atlas.facts))
    return problems


def contradiction_problems(facts: Sequence[Fact]) -> List[str]:
    """Same subject/predicate/context asserted with different values."""

    seen: Dict[Tuple[str, str, str], Fact] = {}
    problems: List[str] = []
    for fact in facts:
        key = (fact.subject, fact.predicate, fact.context or "")
        prior = seen.get(key)
        if prior is None:
            seen[key] = fact
            continue
        if prior.value != fact.value:
            problems.append(
                "contradictory facts for key "
                f"subject={key[0]} predicate={key[1]} context={key[2] or '-'}: "
                f"{prior.id!r} says {prior.value!r} but {fact.id!r} says {fact.value!r}"
            )
    return problems


# --- query -----------------------------------------------------------------


@dataclass(frozen=True)
class QueryResult:
    query: Query
    subsystem: Entity
    entities: Tuple[Entity, ...]
    facts: Tuple[Fact, ...]
    relationships: Tuple[Relationship, ...]
    hypotheses: Tuple[Hypothesis, ...]
    evidence: Tuple[Evidence, ...]


def resolve_query(atlas: Atlas, identity: str) -> Query:
    query = atlas.queries.get(identity)
    if query is not None:
        return query
    matches = sorted(q.id for q in atlas.queries.values() if q.subsystem == identity)
    if len(matches) == 1:
        return atlas.queries[matches[0]]
    if matches:
        raise QueryNotFound(
            f"subsystem {identity!r} has several queries: {', '.join(matches)}"
        )
    known = ", ".join(sorted(atlas.queries)) or "(none)"
    raise QueryNotFound(
        f"unknown query or subsystem {identity!r}; known queries: {known}"
    )


def run_query(atlas: Atlas, identity: str) -> QueryResult:
    query = resolve_query(atlas, identity)
    subsystem = atlas.entities[query.subsystem]
    members = {
        entity.id: entity
        for entity in atlas.entities.values()
        if entity.subsystem == subsystem.id
    }

    def entity_key(entity: Entity) -> Tuple[int, int, str]:
        return (ENTITY_RANK[entity.kind], entity.order, entity.id)

    entities = tuple(sorted(members.values(), key=entity_key))
    order_of = {entity.id: entity_key(entity) for entity in entities}

    facts = tuple(
        sorted(
            (f for f in atlas.facts if f.subject in members),
            key=lambda f: (order_of[f.subject], f.predicate, f.context or "", f.id),
        )
    )
    relationships = tuple(
        sorted(
            (
                r
                for r in atlas.relationships
                if r.source in members and r.target in members
            ),
            key=lambda r: (r.order, r.id),
        )
    )
    hypotheses = tuple(
        sorted(
            (h for h in atlas.hypotheses if h.subject in members),
            key=lambda h: (order_of[h.subject], h.id),
        )
    )

    cited = set()
    for fact in facts:
        cited.update(fact.evidence)
    for hypothesis in hypotheses:
        cited.update(hypothesis.evidence)
    for relationship in relationships:
        cited.update(relationship.evidence)
    evidence = tuple(
        sorted(
            (atlas.evidence[eid] for eid in cited),
            key=lambda e: (EVIDENCE_KINDS.index(e.kind), e.id),
        )
    )

    return QueryResult(
        query=query,
        subsystem=subsystem,
        entities=entities,
        facts=facts,
        relationships=relationships,
        hypotheses=hypotheses,
        evidence=evidence,
    )


# --- rendering -------------------------------------------------------------


def _label(classification: str) -> str:
    return f"[{classification}]"


def _tail(*parts: Optional[str]) -> str:
    kept = [part for part in parts if part]
    return ("  " + "  ".join(kept)) if kept else ""


def render_query_text(atlas: Atlas, result: QueryResult) -> str:
    lines: List[str] = []
    lines.append(f"atlas: {atlas.root} ({len(atlas.packages)} package(s))")
    for file_name, package in atlas.packages:
        lines.append(f"  package {package} ({file_name})")
    lines.append(f"query: {result.query.id}")
    lines.append(f"subsystem: {result.subsystem.id} — {result.subsystem.name}")
    lines.append(f"question: {result.query.question}")
    lines.append(f"answer: {result.query.answer}")

    lines.append("")
    lines.append(f"entities ({len(result.entities)})")
    kind_width = max((len(e.kind) for e in result.entities), default=0)
    id_width = max((len(e.id) for e in result.entities), default=0)
    for entity in result.entities:
        address = f"{entity.address} " if entity.address else ""
        space = f"({entity.space}) " if entity.space else ""
        lines.append(
            f"  {entity.kind:<{kind_width}}  {entity.id:<{id_width}}  "
            f"{address}{space}{entity.name}"
        )

    lines.append("")
    lines.append(f"facts ({len(result.facts)})")
    for fact in result.facts:
        context = f"@{fact.context}" if fact.context else ""
        lines.append(
            f"  {_label(fact.classification)} {fact.subject} {fact.predicate}{context}"
            f" = {fact.value}"
            + _tail(
                f"note: {fact.note}" if fact.note else None,
                f"ev: {','.join(fact.evidence)}",
            )
        )

    lines.append("")
    lines.append(f"causal chain ({len(result.relationships)})")
    for index, relationship in enumerate(result.relationships, start=1):
        context = f"@{relationship.context}" if relationship.context else ""
        lines.append(
            f"  {index:>2}. {relationship.source} -{relationship.kind}-> "
            f"{relationship.target}{context} {_label(relationship.classification)}"
            + _tail(
                relationship.detail,
                f"ev: {','.join(relationship.evidence)}" if relationship.evidence else None,
            )
        )

    lines.append("")
    lines.append(f"semantics and retained uncertainty ({len(result.hypotheses)})")
    for hypothesis in result.hypotheses:
        lines.append(
            f"  {_label(hypothesis.classification)} {hypothesis.subject}: "
            f"{hypothesis.statement}"
            + _tail(
                "alternatives: " + "; ".join(hypothesis.alternatives)
                if hypothesis.alternatives
                else None,
                f"resolve by: {hypothesis.resolves_when}"
                if hypothesis.resolves_when
                else None,
                f"ev: {','.join(hypothesis.evidence)}" if hypothesis.evidence else None,
            )
        )

    lines.append("")
    lines.append(f"evidence ({len(result.evidence)})")
    for item in result.evidence:
        counts = (
            " " + " ".join(f"{k}={item.counts[k]}" for k in sorted(item.counts))
            if item.counts
            else ""
        )
        lines.append(
            f"  {item.id} [{item.kind}] {item.ref}{counts}"
            + _tail(item.detail)
        )
    return "\n".join(lines) + "\n"


def query_json(atlas: Atlas, result: QueryResult) -> Dict[str, Any]:
    def drop_empty(obj: Dict[str, Any]) -> Dict[str, Any]:
        return {k: v for k, v in obj.items() if v not in (None, (), [], {})}

    return {
        "atlas_version": ATLAS_VERSION,
        "packages": [pkg for _, pkg in atlas.packages],
        "query": result.query.id,
        "subsystem": result.subsystem.id,
        "question": result.query.question,
        "answer": result.query.answer,
        "entities": [
            drop_empty(
                {
                    "id": e.id,
                    "kind": e.kind,
                    "name": e.name,
                    "address": e.address,
                    "space": e.space,
                    "detail": e.detail,
                }
            )
            for e in result.entities
        ],
        "facts": [
            drop_empty(
                {
                    "id": f.id,
                    "subject": f.subject,
                    "predicate": f.predicate,
                    "context": f.context,
                    "value": f.value,
                    "classification": f.classification,
                    "note": f.note,
                    "evidence": list(f.evidence),
                }
            )
            for f in result.facts
        ],
        "chain": [
            drop_empty(
                {
                    "id": r.id,
                    "order": r.order,
                    "from": r.source,
                    "kind": r.kind,
                    "to": r.target,
                    "context": r.context,
                    "classification": r.classification,
                    "detail": r.detail,
                    "evidence": list(r.evidence),
                }
            )
            for r in result.relationships
        ],
        "hypotheses": [
            drop_empty(
                {
                    "id": h.id,
                    "subject": h.subject,
                    "statement": h.statement,
                    "classification": h.classification,
                    "alternatives": list(h.alternatives),
                    "resolves_when": h.resolves_when,
                    "evidence": list(h.evidence),
                }
            )
            for h in result.hypotheses
        ],
        "evidence": [
            drop_empty(
                {
                    "id": v.id,
                    "kind": v.kind,
                    "ref": v.ref,
                    "detail": v.detail,
                    "counts": dict(v.counts),
                }
            )
            for v in result.evidence
        ],
    }


def render_validate_text(atlas: Atlas) -> str:
    lines = [f"atlas: {atlas.root}"]
    for file_name, package in atlas.packages:
        lines.append(f"  package {package} ({file_name})")
    lines.append(
        "ok: "
        f"{len(atlas.entities)} entities, {len(atlas.evidence)} evidence, "
        f"{len(atlas.facts)} facts, {len(atlas.hypotheses)} hypotheses, "
        f"{len(atlas.relationships)} relationships, {len(atlas.queries)} queries"
    )
    return "\n".join(lines) + "\n"


def render_list_text(atlas: Atlas) -> str:
    lines = [f"atlas: {atlas.root} ({len(atlas.packages)} package(s))"]
    for query_id in sorted(atlas.queries):
        query = atlas.queries[query_id]
        lines.append(f"  {query.id} [{query.subsystem}] {query.question}")
    return "\n".join(lines) + "\n"


# --- command line ----------------------------------------------------------


def _loaded(root: Path) -> Atlas:
    atlas = load_atlas(root)
    problems = link_problems(atlas)
    if problems:
        raise AtlasError("\n".join(f"error: {problem}" for problem in problems))
    return atlas


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate and query the observatory persistent game atlas."
    )
    parser.add_argument(
        "--root",
        default=str(DEFAULT_ROOT),
        help=f"atlas package directory (default: {DEFAULT_ROOT})",
    )
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("validate", help="check every package, reference, and fact key")
    sub.add_parser("list", help="list query identities")
    query = sub.add_parser("query", help="answer one subsystem/query identity")
    query.add_argument("identity", help="query id or subsystem id")
    query.add_argument(
        "--json", action="store_true", help="emit compact JSON instead of text"
    )
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    root = Path(args.root)
    try:
        atlas = _loaded(root)
        if args.command == "validate":
            sys.stdout.write(render_validate_text(atlas))
            return 0
        if args.command == "list":
            sys.stdout.write(render_list_text(atlas))
            return 0
        result = run_query(atlas, args.identity)
    except QueryNotFound as exc:
        print(f"error: {exc}", file=sys.stderr)
        return USAGE_EXIT
    except AtlasError as exc:
        message = str(exc)
        if not message.startswith("error:"):
            message = f"error: {message}"
        print(message, file=sys.stderr)
        return FAILURE_EXIT
    if args.json:
        sys.stdout.write(
            json.dumps(
                query_json(atlas, result), sort_keys=True, separators=(",", ":")
            )
            + "\n"
        )
    else:
        sys.stdout.write(render_query_text(atlas, result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
