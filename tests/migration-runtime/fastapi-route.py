from fastapi import APIRouter

router = APIRouter()


@router.get("/metrics/{metric_id}")
async def get_metric(metric_id: int):
    return {"metric_id": metric_id, "scaled": metric_id * 4}
