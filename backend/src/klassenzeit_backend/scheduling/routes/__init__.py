"""Scheduling route collection."""

from fastapi import APIRouter

from klassenzeit_backend.scheduling.routes.lessons import generate_router as lessons_generate_router
from klassenzeit_backend.scheduling.routes.lessons import router as lessons_router
from klassenzeit_backend.scheduling.routes.placements import router as placements_router
from klassenzeit_backend.scheduling.routes.rooms import router as rooms_router
from klassenzeit_backend.scheduling.routes.schedule import router as schedule_router
from klassenzeit_backend.scheduling.routes.school_classes import router as school_classes_router
from klassenzeit_backend.scheduling.routes.stundentafeln import router as stundentafeln_router
from klassenzeit_backend.scheduling.routes.subjects import router as subjects_router
from klassenzeit_backend.scheduling.routes.teachers import router as teachers_router
from klassenzeit_backend.scheduling.routes.week_schemes import router as week_schemes_router

scheduling_router = APIRouter()
scheduling_router.include_router(subjects_router)
scheduling_router.include_router(week_schemes_router)
scheduling_router.include_router(rooms_router)
scheduling_router.include_router(schedule_router)
scheduling_router.include_router(teachers_router)
scheduling_router.include_router(stundentafeln_router)
scheduling_router.include_router(school_classes_router)
scheduling_router.include_router(lessons_router)
scheduling_router.include_router(lessons_generate_router)
scheduling_router.include_router(placements_router)
