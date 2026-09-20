# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

# Django imports
from django.conf import settings
from django.http import HttpRequest

# Third party imports
from rest_framework.request import Request

# Module imports
from plane.utils.ip_address import get_client_ip


def base_host(
    request: Request | HttpRequest,
    is_admin: bool = False,
    is_space: bool = False,
    is_app: bool = False,
) -> str:
    """Utility function to return host / origin from the request"""
    # Calculate the base origin from request
    base_origin = settings.WEB_URL or settings.APP_BASE_URL

    # Admin redirection
    if is_admin:
        admin_base_path = getattr(settings, "ADMIN_BASE_PATH", None)
        if not isinstance(admin_base_path, str):
            admin_base_path = "/god-mode/"
        if not admin_base_path.startswith("/"):
            admin_base_path = "/" + admin_base_path
        if not admin_base_path.endswith("/"):
            admin_base_path += "/"

        if settings.ADMIN_BASE_URL:
            return settings.ADMIN_BASE_URL + admin_base_path
        else:
            return base_origin + admin_base_path

    # Space redirection
    if is_space:
        space_base_path = getattr(settings, "SPACE_BASE_PATH", None)
        if not isinstance(space_base_path, str):
            space_base_path = "/spaces/"
        if not space_base_path.startswith("/"):
            space_base_path = "/" + space_base_path
        if not space_base_path.endswith("/"):
            space_base_path += "/"

        if settings.SPACE_BASE_URL:
            return settings.SPACE_BASE_URL + space_base_path
        else:
            return base_origin + space_base_path

    # App Redirection
    if is_app:
        return resolve_app_host(request)

    return base_origin


def resolve_app_host(request: Request | HttpRequest) -> str:
    """Return the app origin the user should land on.

    Prefers the request's Origin header when it is one of the trusted
    (CORS/CSRF-allowed) origins, so deployments reachable from multiple
    hosts (e.g. a tunnel domain and a LAN IP) redirect back to whichever
    host the user actually came from. Falls back to APP_BASE_URL, then
    WEB_URL.
    """
    origin = request.headers.get("Origin")
    if origin:
        origin = origin.rstrip("/")
        allowed_origins = getattr(settings, "CORS_ALLOWED_ORIGINS", None) or []
        if origin in allowed_origins:
            return origin

    if settings.APP_BASE_URL:
        return settings.APP_BASE_URL

    return settings.WEB_URL or settings.APP_BASE_URL


def user_ip(request: Request | HttpRequest) -> str:
    return get_client_ip(request=request)
