#!/usr/bin/env python3
# Tests for tools/observatory_atlas.py. Licensed under LICENSE.md.

"""Focused tests for the observatory persistent game atlas."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import observatory_atlas as oa  # noqa: E402

TRACKED_ROOT = oa.DEFAULT_ROOT


def minimal_package(**overrides: object) -> dict:
    """One structurally valid package that tests bend into invalid shapes."""

    package = {
        "atlas_version": 1,
        "package": "unit-test",
        "evidence": [
            {"id": "ev.doc", "kind": "tracked-document", "ref": "docs/UNIT.md"}
        ],
        "entities": [
            {"id": "sub", "kind": "subsystem", "name": "unit subsystem"},
            {
                "id": "exp.positive",
                "kind": "experiment",
                "name": "positive experiment",
                "subsystem": "sub",
            },
            {
                "id": "cell",
                "kind": "state-cell",
                "name": "unit cell",
                "subsystem": "sub",
                "address": ">1D00",
            },
            {
                "id": "op",
                "kind": "operation",
                "name": "unit op",
                "subsystem": "sub",
                "address": ">66A7",
            },
        ],
        "facts": [
            {
                "id": "fact.one",
                "subject": "cell",
                "predicate": "initial-value",
                "context": "exp.positive",
                "value": ">06",
                "classification": "observed",
                "evidence": ["ev.doc"],
            }
        ],
        "relationships": [
            {
                "id": "rel.one",
                "order": 1,
                "from": "op",
                "kind": "compares",
                "to": "cell",
                "classification": "observed",
                "evidence": ["ev.doc"],
            }
        ],
        "queries": [
            {
                "id": "q.unit",
                "subsystem": "sub",
                "question": "unit question?",
                "answer": "unit answer.",
            }
        ],
    }
    package.update(overrides)
    return package


class TempAtlas:
    """A throwaway atlas root holding one or more packages."""

    def __init__(self, *packages: dict) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        for index, package in enumerate(packages, start=1):
            path = self.root / f"{index:04d}-{package['package']}.json"
            path.write_text(json.dumps(package, indent=2), encoding="utf-8")

    def __enter__(self) -> "TempAtlas":
        return self

    def __exit__(self, *_exc: object) -> None:
        self._tmp.cleanup()

    def problems(self) -> list:
        return oa.link_problems(oa.load_atlas(self.root))


class SeededAtlasTest(unittest.TestCase):
    """The tracked atlas validates and answers its accepted queries."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.atlas = oa.load_atlas(TRACKED_ROOT)
        cls.problems = oa.link_problems(cls.atlas)

    def test_tracked_atlas_validates(self) -> None:
        self.assertEqual([], self.problems)
        self.assertGreaterEqual(len(self.atlas.packages), 2)

    def test_query_resolves_by_subsystem_and_by_query_id(self) -> None:
        by_subsystem = oa.resolve_query(self.atlas, "tod.stairs-descend")
        by_id = oa.resolve_query(self.atlas, by_subsystem.id)
        self.assertEqual(by_subsystem, by_id)

    def test_stairs_query_reconstructs_the_accepted_chain(self) -> None:
        text = oa.render_query_text(
            self.atlas, oa.run_query(self.atlas, "tod.stairs-descend")
        )
        for needle in (
            ">8375",  # key byte, identical in both runs
            ">0A",
            ">1D00",  # distinguishing predicate, positive 06 / negative 05
            ">06",
            ">05",
            ">66A7",  # positive continuation
            ">66AC",  # negative branch site
            ">663F",  # negative branch target
            ">66C7",  # transition mutation
            ">1CF8 00→01",
            ">66CB",  # predicate store
            ">1D00 06→05",
            ">A798",  # later copy
            ">10FA 00→01",
            ">08B0",
            ">D013",
            ">08CE",
            ">D020",
            ">1D2A",
            ">D802",
            "DESCENDING",
            "positive outcome",
            "negative outcome",
        ):
            self.assertIn(needle, text, f"query output lost {needle}")

    def test_stairs_query_keeps_every_evidence_label(self) -> None:
        result = oa.run_query(self.atlas, "tod.stairs-descend")
        fact_labels = {fact.classification for fact in result.facts}
        self.assertEqual({"observed", "source-confirmed", "corroborated"}, fact_labels)
        semantic_labels = {item.classification for item in result.hypotheses}
        self.assertEqual({"inferred", "unresolved"}, semantic_labels)
        # The retained uncertainties the R2 result refused to promote.
        subjects = {item.subject for item in result.hypotheses}
        self.assertIn("tod.state.vram-1D00", subjects)
        self.assertIn("tod.state.vram-1CE8", subjects)
        self.assertIn("tod.state.vram-10FE", subjects)

    def test_stairs_query_includes_the_accepted_r4_model_comparison(self) -> None:
        text = oa.render_query_text(
            self.atlas, oa.run_query(self.atlas, "tod.stairs-descend")
        )
        for needle in (
            "tod.model.stairs-descend",
            "22 compared in-boundary fields",
            ">66B3",
            ">1CE8 was 00",
            "without a >10FE read",
            "91-record filtered state captures",
            "684-record name-table VDP captures",
        ):
                self.assertIn(needle, text, f"query output lost R4 evidence {needle}")

    def test_stairs_fallback_query_reconstructs_g007_contract(self) -> None:
        text = oa.render_query_text(
            self.atlas, oa.run_query(self.atlas, "tod.stairs-fallback")
        )
        for needle in (
            ">8018",
            ">96EA",
            ">83A1",
            ">8379",
            ">96FF",
            ">1D01",
            ">9710",
            ">66C5",
            ">663F",
            "new key >20",
            "continuation_allowed",
            "not a game-state predicate",
        ):
            self.assertIn(needle, text, f"query output lost G-007 evidence {needle}")

    def test_dungeon_query_resolves_by_subsystem_and_by_query_id(self) -> None:
        by_subsystem = oa.resolve_query(self.atlas, "tod.dungeon-generation")
        by_id = oa.resolve_query(self.atlas, by_subsystem.id)
        self.assertEqual(by_subsystem, by_id)

    def test_dungeon_query_reconstructs_the_reconnaissance_result(self) -> None:
        text = oa.render_query_text(
            self.atlas, oa.run_query(self.atlas, "tod.dungeon-generation")
        )
        for needle in (
            ">62F4",  # outer generation routine
            ">63E3",  # observed per-floor call
            ">8002",  # call stub
            ">8246",  # per-floor entry
            ">34B8..>36D1",  # 538-byte candidate payload
            ">83C0",  # controlled random seed
            ">0000",
            ">1234",
            "220",  # final candidate bytes changed by the seed variation
            ">8611",  # invariant reset operations
            ">863D",
            ">8553",  # seed-dependent value-store operation
            ">8339",  # stride-32 stores
            ">83D8",  # stride-1 stores
            ">1D2A",  # native VDP writer
            ">D802",
            "GENERAL STORE",  # bounded visible completion
            "not yet economical",
        ):
            self.assertIn(needle, text, f"query output lost reconnaissance {needle}")

    def test_dungeon_query_keeps_semantics_out_of_facts(self) -> None:
        result = oa.run_query(self.atlas, "tod.dungeon-generation")
        fact_labels = {fact.classification for fact in result.facts}
        self.assertEqual({"observed", "source-confirmed"}, fact_labels)
        semantic_labels = {item.classification for item in result.hypotheses}
        self.assertEqual({"inferred", "unresolved"}, semantic_labels)
        statements = "\n".join(item.statement for item in result.hypotheses)
        self.assertIn("topology grid", statements)
        self.assertIn("exact graphical or gameplay meanings", statements)

    def test_stairs_query_cites_documents_and_owner_local_experiments(self) -> None:
        result = oa.run_query(self.atlas, "tod.stairs-descend")
        kinds = {item.kind for item in result.evidence}
        self.assertIn("tracked-document", kinds)
        self.assertIn("owner-local-experiment", kinds)
        for fact in result.facts:
            self.assertTrue(fact.evidence, f"{fact.id} cites no evidence")

    def test_stairs_query_json_is_importable(self) -> None:
        result = oa.run_query(self.atlas, "tod.stairs-descend")
        payload = json.dumps(
            oa.query_json(self.atlas, result), sort_keys=True, separators=(",", ":")
        )
        reparsed = json.loads(payload)
        self.assertEqual(len(result.relationships), len(reparsed["chain"]))
        self.assertEqual(len(result.facts), len(reparsed["facts"]))

    def test_seed_stores_no_owner_paths_or_media(self) -> None:
        for path in sorted(TRACKED_ROOT.glob("*.json")):
            raw = path.read_text(encoding="utf-8")
            for forbidden in ("/Users/", "C:\\", "~/", ".rpk", ".dsk", ".ctg", ".png"):
                self.assertNotIn(forbidden, raw, f"{path.name} leaks {forbidden}")


class NoContextCliTest(unittest.TestCase):
    """A fresh task gets the answer from the atlas alone."""

    def _run(self, *args: str, cwd: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(TOOLS / "observatory_atlas.py"), *args],
            cwd=str(cwd),
            capture_output=True,
            text=True,
            check=False,
        )

    def test_validate_and_query_from_an_unrelated_directory(self) -> None:
        with tempfile.TemporaryDirectory() as elsewhere:
            here = Path(elsewhere)
            validated = self._run("validate", cwd=here)
            self.assertEqual(0, validated.returncode, validated.stderr)

            queried = self._run("query", "tod.stairs-descend", cwd=here)
            self.assertEqual(0, queried.returncode, queried.stderr)
            # No repository context, no traces: only the tracked atlas.
            self.assertIn(">1D00", queried.stdout)
            self.assertIn(">663F", queried.stdout)
            self.assertIn(">10FA 00→01", queried.stdout)

            dungeon = self._run("query", "tod.dungeon-generation", cwd=here)
            self.assertEqual(0, dungeon.returncode, dungeon.stderr)
            # The bounded generator result is likewise recoverable from the
            # tracked atlas alone, with no owner-local experiment available.
            self.assertIn(">62F4", dungeon.stdout)
            self.assertIn(">34B8..>36D1", dungeon.stdout)
            self.assertIn("changes 220 payload bytes", dungeon.stdout)

    def test_unknown_query_identity_is_a_usage_error(self) -> None:
        with tempfile.TemporaryDirectory() as elsewhere:
            done = self._run("query", "tod.no-such-thing", cwd=Path(elsewhere))
            self.assertEqual(oa.USAGE_EXIT, done.returncode)
            self.assertIn("tod.no-such-thing", done.stderr)
            self.assertIn("known queries", done.stderr)


class InvalidReferenceTest(unittest.TestCase):
    def test_unknown_fact_subject(self) -> None:
        package = minimal_package()
        package["facts"][0]["subject"] = "cell-typo"
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("fact.one", problems[0])
        self.assertIn("cell-typo", problems[0])

    def test_unknown_evidence_reference(self) -> None:
        package = minimal_package()
        package["relationships"][0]["evidence"] = ["ev.missing"]
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("rel.one", problems[0])
        self.assertIn("ev.missing", problems[0])

    def test_unknown_context_reference(self) -> None:
        package = minimal_package()
        package["facts"][0]["context"] = "exp.missing"
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("fact.one", problems[0])
        self.assertIn("exp.missing", problems[0])

    def test_context_must_identify_an_experiment(self) -> None:
        package = minimal_package()
        package["facts"][0]["context"] = "cell"
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("fact.one", problems[0])
        self.assertIn("not an experiment", problems[0])

    def test_unknown_relationship_endpoint(self) -> None:
        package = minimal_package()
        package["relationships"][0]["to"] = "cell-gone"
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertTrue(any("cell-gone" in problem for problem in problems), problems)
        self.assertTrue(any("rel.one" in problem for problem in problems), problems)

    def test_unknown_query_subsystem(self) -> None:
        package = minimal_package()
        package["queries"][0]["subsystem"] = "sub-typo"
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("q.unit", problems[0])
        self.assertIn("sub-typo", problems[0])

    def test_entity_subsystem_must_be_a_subsystem(self) -> None:
        package = minimal_package()
        next(item for item in package["entities"] if item["id"] == "cell")[
            "subsystem"
        ] = "op"
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("'cell'", problems[0])
        self.assertIn("not a subsystem", problems[0])

    def test_cli_reports_the_offending_reference(self) -> None:
        package = minimal_package()
        package["facts"][0]["evidence"] = ["ev.nope"]
        with TempAtlas(package) as atlas:
            done = subprocess.run(
                [
                    sys.executable,
                    str(TOOLS / "observatory_atlas.py"),
                    "--root",
                    str(atlas.root),
                    "validate",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(oa.FAILURE_EXIT, done.returncode)
        self.assertIn("ev.nope", done.stderr)
        self.assertIn("fact.one", done.stderr)


class ContradictionTest(unittest.TestCase):
    def _second_fact(self, **overrides: object) -> dict:
        fact = {
            "id": "fact.two",
            "subject": "cell",
            "predicate": "initial-value",
            "context": "exp.positive",
            "value": ">06",
            "classification": "corroborated",
            "evidence": ["ev.doc"],
        }
        fact.update(overrides)
        return fact

    def test_identical_corroborating_assertions_coexist(self) -> None:
        package = minimal_package()
        package["facts"].append(self._second_fact())
        with TempAtlas(package) as atlas:
            self.assertEqual([], atlas.problems())

    def test_same_key_with_a_different_value_is_refused(self) -> None:
        package = minimal_package()
        package["facts"].append(self._second_fact(value=">05"))
        with TempAtlas(package) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("subject=cell", problems[0])
        self.assertIn("predicate=initial-value", problems[0])
        self.assertIn("context=exp.positive", problems[0])
        self.assertIn("fact.one", problems[0])
        self.assertIn("fact.two", problems[0])

    def test_a_different_context_is_not_a_contradiction(self) -> None:
        package = minimal_package()
        package["entities"].append(
            {
                "id": "exp.negative",
                "kind": "experiment",
                "name": "negative experiment",
                "subsystem": "sub",
            }
        )
        package["facts"].append(
            self._second_fact(context="exp.negative", value=">05")
        )
        with TempAtlas(package) as atlas:
            self.assertEqual([], atlas.problems())

    def test_contradiction_across_appended_packages_is_refused(self) -> None:
        first = minimal_package()
        second = minimal_package(
            package="unit-test-later",
            evidence=[],
            entities=[],
            facts=[self._second_fact(value=">05")],
            relationships=[],
            queries=[],
        )
        with TempAtlas(first, second) as atlas:
            problems = atlas.problems()
        self.assertEqual(1, len(problems), problems)
        self.assertIn("fact.two", problems[0])


class StructuralRefusalTest(unittest.TestCase):
    def _load_error(self, *packages: dict) -> str:
        with TempAtlas(*packages) as atlas:
            with self.assertRaises(oa.AtlasError) as caught:
                oa.load_atlas(atlas.root)
        return str(caught.exception)

    def test_duplicate_id_within_a_package(self) -> None:
        package = minimal_package()
        package["facts"].append(dict(package["facts"][0]))
        message = self._load_error(package)
        self.assertIn("duplicate id 'fact.one'", message)

    def test_duplicate_id_across_appended_packages(self) -> None:
        first = minimal_package()
        second = minimal_package(
            package="unit-test-later",
            evidence=[],
            entities=[
                {
                    "id": "cell",
                    "kind": "state-cell",
                    "name": "clashing cell",
                    "subsystem": "sub",
                }
            ],
            facts=[],
            relationships=[],
            queries=[],
        )
        message = self._load_error(first, second)
        self.assertIn("duplicate id 'cell'", message)
        self.assertIn("unit-test-later", message)

    def test_unsupported_fact_classification(self) -> None:
        package = minimal_package()
        package["facts"][0]["classification"] = "probably"
        message = self._load_error(package)
        self.assertIn("facts[fact.one]", message)
        self.assertIn("unsupported classification 'probably'", message)

    def test_hypotheses_reject_factual_classifications(self) -> None:
        package = minimal_package(
            hypotheses=[
                {
                    "id": "hyp.one",
                    "subject": "cell",
                    "statement": "a semantic guess",
                    "classification": "observed",
                    "evidence": ["ev.doc"],
                }
            ]
        )
        message = self._load_error(package)
        self.assertIn("hypotheses[hyp.one]", message)
        self.assertIn("unsupported classification 'observed'", message)

    def test_unsupported_entity_kind(self) -> None:
        package = minimal_package()
        next(item for item in package["entities"] if item["id"] == "cell")[
            "kind"
        ] = "database-table"
        message = self._load_error(package)
        self.assertIn("entities[cell]", message)
        self.assertIn("unsupported kind 'database-table'", message)

    def test_unsupported_relationship_kind(self) -> None:
        package = minimal_package()
        package["relationships"][0]["kind"] = "vibes"
        message = self._load_error(package)
        self.assertIn("relationships[rel.one]", message)
        self.assertIn("unsupported kind 'vibes'", message)

    def test_unsupported_atlas_version(self) -> None:
        package = minimal_package(atlas_version=99)
        message = self._load_error(package)
        self.assertIn("atlas_version must be 1", message)

    def test_missing_required_field(self) -> None:
        package = minimal_package()
        del package["facts"][0]["value"]
        message = self._load_error(package)
        self.assertIn("facts[fact.one]", message)
        self.assertIn("'value'", message)


if __name__ == "__main__":
    unittest.main()
