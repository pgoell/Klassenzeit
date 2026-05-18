"""Audit-log capture for super-admin cross-school writes (item 10g)."""

import uuid

from klassenzeit_backend.auth.dependencies import is_super_admin
from klassenzeit_backend.db.models.user import User

WRITE_METHODS: frozenset[str] = frozenset({"POST", "PATCH", "PUT", "DELETE"})
SCHOOLS_ROUTE_PREFIX = "/api/schools"


def should_audit_request(
    user: User | None,
    target_school_id: uuid.UUID | None,
    method: str,
    route_template: str,
) -> bool:
    """Return True iff the request requires audit-log capture.

    Captures writes performed by super-admins where the elevation was
    actually used: target school is not in the user's directly-accessible
    schools (home + memberships), OR the route is under /api/schools.
    """
    if user is None or not is_super_admin(user):
        return False
    if method not in WRITE_METHODS:
        return False
    if route_template.startswith(SCHOOLS_ROUTE_PREFIX):
        return True
    if target_school_id is None:
        return False
    home = user.school_id
    memberships = user.memberships or []
    accessible = {home} | {m.school_id for m in memberships}
    return target_school_id not in accessible
