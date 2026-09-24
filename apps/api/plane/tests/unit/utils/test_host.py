# Copyright (c) 2023-present Plane Software, Inc. and contributors
# SPDX-License-Identifier: AGPL-3.0-only
# See the LICENSE file for details.

import pytest
from django.test import RequestFactory

from plane.authentication.utils.host import resolve_app_host


@pytest.mark.unit
class TestResolveAppHost:
    """resolve_app_host prefers a trusted request Origin over APP_BASE_URL"""

    def setup_method(self):
        self.factory = RequestFactory()

    def test_returns_app_base_url_when_no_origin(self, settings):
        settings.APP_BASE_URL = "http://192.168.1.11:3000"
        settings.CORS_ALLOWED_ORIGINS = ["https://dashboard.terraline.space"]
        request = self.factory.post("/auth/sign-out/")

        assert resolve_app_host(request) == "http://192.168.1.11:3000"

    def test_returns_trusted_origin(self, settings):
        settings.APP_BASE_URL = "http://192.168.1.11:3000"
        settings.CORS_ALLOWED_ORIGINS = ["https://dashboard.terraline.space", "http://192.168.1.11:3000"]
        request = self.factory.post("/auth/sign-out/", HTTP_ORIGIN="https://dashboard.terraline.space")

        assert resolve_app_host(request) == "https://dashboard.terraline.space"

    def test_ignores_untrusted_origin(self, settings):
        settings.APP_BASE_URL = "http://192.168.1.11:3000"
        settings.CORS_ALLOWED_ORIGINS = ["https://dashboard.terraline.space"]
        request = self.factory.post("/auth/sign-out/", HTTP_ORIGIN="https://evil.example.com")

        assert resolve_app_host(request) == "http://192.168.1.11:3000"

    def test_falls_back_to_web_url_when_app_base_url_unset(self, settings):
        settings.WEB_URL = "https://dashboard.terraline.space"
        settings.APP_BASE_URL = None
        settings.CORS_ALLOWED_ORIGINS = []
        request = self.factory.post("/auth/sign-out/")

        assert resolve_app_host(request) == "https://dashboard.terraline.space"

    def test_strips_trailing_slash_from_origin(self, settings):
        settings.APP_BASE_URL = "http://192.168.1.11:3000"
        settings.CORS_ALLOWED_ORIGINS = ["https://dashboard.terraline.space"]
        request = self.factory.post("/auth/sign-out/", HTTP_ORIGIN="https://dashboard.terraline.space/")

        assert resolve_app_host(request) == "https://dashboard.terraline.space"
