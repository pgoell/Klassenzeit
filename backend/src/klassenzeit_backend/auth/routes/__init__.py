"""Auth router — collects all auth sub-routers."""

from fastapi import APIRouter

from klassenzeit_backend.auth.routes.admin import router as admin_router
from klassenzeit_backend.auth.routes.audit_log import router as audit_log_router
from klassenzeit_backend.auth.routes.login import router as login_router
from klassenzeit_backend.auth.routes.me import router as me_router
from klassenzeit_backend.auth.routes.schools import router as schools_router
from klassenzeit_backend.auth.routes.switch_school import router as switch_school_router

auth_router = APIRouter()
auth_router.include_router(login_router)
auth_router.include_router(me_router)
auth_router.include_router(admin_router)
auth_router.include_router(audit_log_router)
auth_router.include_router(schools_router)
auth_router.include_router(switch_school_router)
