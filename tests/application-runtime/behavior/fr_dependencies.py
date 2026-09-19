from fastapi import HTTPException, Request


def load_session(request: Request):
    if request.query_params.get("token") != "present":
        raise HTTPException(status_code=401)
