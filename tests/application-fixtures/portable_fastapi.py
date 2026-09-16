from fastapi import FastAPI
from fastapi.responses import JSONResponse

app = FastAPI()


@app.get("/fast/{id}")
def show_fast(id: str):
    return JSONResponse(content={"id": id, "framework": "portable"}, status_code=201)
