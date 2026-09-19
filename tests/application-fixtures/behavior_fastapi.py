from fastapi import Depends, FastAPI, Security
from fastapi.responses import JSONResponse

app = FastAPI()


@app.middleware("http")
async def audit(request, call_next):
    return await call_next(request)


app.add_middleware(AuthMiddleware)


def load_session():
    return "s"


def load_terms():
    return []


@app.post("/records/{record_id}")
def create_record(
    record_id: str,
    session=Security(load_session),
    terms=Depends(load_terms),
):
    return JSONResponse(content={"id": record_id}, status_code=201)
