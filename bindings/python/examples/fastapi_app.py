from typing import Annotated, Any

from fastapi import FastAPI, Header, HTTPException

import polyui

ACCOUNTS = {"u1": {"gas": "**** 0083", "power": "**** 4471"}}

ACTIONS = {
    "pay_bill": {
        "args": {"bill": {"kind": "string", "required": True, "max": 32}},
        "requires_confirm": True,
    }
}


def find_account(user: str | None, source: str, key: str) -> str | None:
    if user not in ACCOUNTS:
        raise polyui.RefRefusedError("unknown user")
    return ACCOUNTS[user].get(key)


validator = polyui.Validator(
    polyui.Catalogs.builtin(),
    actions=ACTIONS,
    ref_sources={"account": {"kind": "string", "max": 40}},
    resolver=find_account,
)

app = FastAPI(title="PolyUI show_ui check")


@app.get("/tools")
def tools(catalogs: str = "core@1,chart@1") -> list[dict[str, Any]]:
    return validator.tool_definitions(catalogs.split(","))


@app.post("/show_ui")
def show_ui(
    payload: dict[str, Any],
    x_user: Annotated[str | None, Header()] = None,
    x_catalogs: Annotated[str, Header()] = "core@1,chart@1",
    x_locale: Annotated[str, Header()] = "en",
) -> dict[str, Any]:
    try:
        prepared = validator.prepare_show(
            payload,
            user=x_user,
            declared=x_catalogs.split(","),
            locale=x_locale,
        )
    except polyui.ValidationError as error:
        raise HTTPException(
            status_code=422,
            detail={"tool_result": str(error), "problems": error.problems},
        ) from error
    return {
        "message": prepared.messages[0],
        "tool_result": prepared.summary,
        "actions": prepared.actions,
    }
