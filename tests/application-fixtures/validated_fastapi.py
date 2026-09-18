from fastapi import Body, FastAPI, Query
from fastapi.responses import JSONResponse

app = FastAPI()


@app.post("/records/{record_id}")
def create_record(
    record_id: str,
    page_size: int = Query(alias="limit"),
    published: bool = Body(embed=True, alias="visible"),
):
    return JSONResponse(
        content={"id": record_id, "limit": page_size, "published": published},
        status_code=201,
    )
