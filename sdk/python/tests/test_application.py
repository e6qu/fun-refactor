from pathlib import Path as FilePath

import pytest

from fr_ir.application import (ApplicationIr, ApplicationNode, Array, HttpInput, HttpRoute, Input,
                               Literal, Object, Path, RouteBundle, StaticComponent, StaticElement,
                               StaticText)
from fr_ir.context import ContextSession
from fr_ir.ir import IrError
from fr_ir.runtime import FrClient


@pytest.mark.parametrize("path", ["relative", "/a/", "/a//b", "/..", "/%61", "/a?b", "/a#b", "/é", "/{x}/{x}", "/{9x}", "/a:b", "/x*", "/[x]"])
def test_path_refusals(path):
    with pytest.raises(IrError):
        HttpRoute("GET", path, 200, Literal(True)).to_data()


@pytest.mark.parametrize("value", [2**53, -(2**53), 1.5, [], {}])
def test_number_and_aggregate_literal_refusals(value):
    with pytest.raises(IrError):
        HttpRoute("GET", "/", 200, Literal(value)).to_data()


def test_native_sdk_authoring_and_progressive_application_access(tmp_path):
    binary = FilePath(__file__).resolve().parents[3] / "target/debug/fr"
    if not binary.is_file():
        pytest.skip("build fr for SDK integration")
    bundle = RouteBundle([
        HttpRoute("GET", "/records/{id}", 200, Object({
            "id": Path("id"), "__proto__": Array([Literal(None), Literal(True)])})),
        HttpRoute("POST", "/audit", 201, Literal("accepted")),
    ])
    bundle.write(tmp_path / "application.json")
    client = FrClient(root=tmp_path, executable=str(binary))
    report = client.call("migrate", "application", "--ir", "application.json", "--to", "fastapi", "--out", "generated", "--write")
    assert report.at("/migration/ir_object_digest") == bundle.object_digest()
    assert report.at("/applied") is True
    structure = client.project("application")
    typed = ApplicationIr.from_data(structure.at("/model"))
    assert typed.object_digest() == structure.at("/object_digest")
    assert RouteBundle.from_data(bundle.to_data()).object_digest() == bundle.object_digest()
    mapped = client.project("map", "--depth", "0")
    disclosure = client.disclose(mapped.at("/root"), view="application")
    assert disclosure.at("/commitment/object_root") == structure.at("/object_digest")
    workspace = ContextSession(client, disclosure)
    revealed = workspace.materialize_section("omissions", max_calls=64)
    assert revealed == structure.at("/model/omissions")
    assert workspace.reveal_section("applications").at("/revealed/object_digest")
    with pytest.raises(FileExistsError):
        bundle.write(tmp_path / "application.json")


def test_expression_and_dispatch_limits():
    expr = Literal(False)
    for _ in range(34):
        expr = Array([expr])
    with pytest.raises(IrError, match="depth"):
        HttpRoute("GET", "/", 200, expr).to_data()
    with pytest.raises(IrError, match="overlap"):
        RouteBundle([HttpRoute("GET", "/x", 200, Literal(True)),
                     HttpRoute("POST", "/{id}", 201, Path("id"))]).to_data()


def test_typed_hierarchy_refuses_duplicate_identities_and_false_claims():
    value = {"schema": "fr-application-ir-1", "revision": "a" * 64,
             "applications": [{"id": "root", "kind": "application", "source": None,
                               "data": {}, "children": []}], "omissions": {}, "runtime_proved": False}
    assert ApplicationIr.from_data(value).to_data() == value
    value["applications"].append(value["applications"][0])
    with pytest.raises(IrError, match="duplicate"):
        ApplicationIr.from_data(value)
    value["applications"].pop()
    value["runtime_proved"] = True
    with pytest.raises(IrError, match="proof"):
        ApplicationIr.from_data(value)


def test_static_component_mirrors_the_bounded_application_ir():
    component = StaticComponent("App", StaticElement("main", {"className": "shell"}, [
        StaticElement("h1", {}, [StaticText("Ready")])]))
    node = ApplicationNode("component", "component", None, {}, [], component=component)
    model = ApplicationIr("a" * 64, [node], {})
    assert ApplicationIr.from_data(model.to_data()).to_data() == model.to_data()


def test_validated_request_ir_matches_the_native_shape():
    route = HttpRoute(
        "POST", "/records/{id}", 201,
        Object({"id": Path("id"), "limit": Input("limit"), "title": Input("title")}),
        (HttpInput("limit", "query", "integer"),
         HttpInput("title", "json-body", "string")),
    )
    value = route.to_data()
    assert value["inputs"] == [
        {"name": "limit", "source": "query", "scalar": "integer"},
        {"name": "title", "source": "json-body", "scalar": "string"},
    ]
    assert HttpRoute.from_data(value).to_data() == value
    assert RouteBundle.from_data(RouteBundle((route,)).to_data()).to_data() == RouteBundle((route,)).to_data()


@pytest.mark.parametrize("route", [
    HttpRoute("GET", "/", 200, Input("body"), (HttpInput("body", "json-body", "string"),)),
    HttpRoute("POST", "/{id}", 200, Input("id"), (HttpInput("id", "query", "string"),)),
    HttpRoute("POST", "/", 200, Input("missing"), (HttpInput("value", "query", "string"),)),
])
def test_validated_request_ir_refuses_unportable_shapes(route):
    with pytest.raises(IrError):
        route.to_data()
    with pytest.raises(IrError, match="attribute"):
        StaticComponent("App", StaticElement("button", {"onClick": "run"}, [])).to_data()
    with pytest.raises(IrError, match="whitespace"):
        StaticComponent("App", StaticElement("p", {}, [StaticText(" padded ")])).to_data()
