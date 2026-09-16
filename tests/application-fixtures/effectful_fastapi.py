from fastapi import FastAPI

app = FastAPI()


@app.get("/records/{id}")
def show(id: str):
    return load_record(id)
