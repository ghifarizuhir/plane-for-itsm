/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { AxiosInstance } from "axios";
import { create } from "axios";
import { env } from "@/env";
import { AppError } from "@/lib/errors";
import { mergeRefreshedCookies } from "@/utils/cookies";

export const buildDefaultHeaders = (webBaseUrl?: string): Record<string, string> => {
  const origin = webBaseUrl?.trim();
  return origin ? { Origin: origin } : {};
};

export abstract class APIService {
  protected baseURL: string;
  private axiosInstance: AxiosInstance;
  private header: Record<string, string> = {};

  constructor(baseURL?: string) {
    this.baseURL = baseURL || env.API_BASE_URL;
    this.axiosInstance = create({
      baseURL: this.baseURL,
      withCredentials: true,
      timeout: 20000,
      headers: { common: buildDefaultHeaders(env.WEB_BASE_URL) },
    });
    this.setupInterceptors();
  }

  private setupInterceptors() {
    this.axiosInstance.interceptors.response.use(
      (response) => response,
      (error) => {
        return Promise.reject(new AppError(error));
      }
    );
  }

  setHeader(key: string, value: string) {
    this.header[key] = value;
  }

  getHeader() {
    return this.header;
  }

  /**
   * Rotate the session using the refresh cookie embedded in `cookie`.
   * Returns the merged `Cookie` header value, or `null` when rotation
   * failed (caller must keep the old cookie and surface the error).
   */
  async refreshAccessCookie(cookie: string): Promise<string | null> {
    try {
      const response = await this.axiosInstance.post("/api/auth/refresh/", null, {
        headers: { Cookie: cookie },
      });
      const setCookies = response.headers?.["set-cookie"] as unknown;
      const list = Array.isArray(setCookies) ? (setCookies as string[]) : [];
      if (list.length === 0) return null;
      return mergeRefreshedCookies(cookie, list);
    } catch {
      return null;
    }
  }

  get(url: string, params = {}, config = {}) {
    return this.axiosInstance.get(url, {
      ...params,
      ...config,
    });
  }

  post(url: string, data = {}, config = {}) {
    return this.axiosInstance.post(url, data, config);
  }

  put(url: string, data = {}, config = {}) {
    return this.axiosInstance.put(url, data, config);
  }

  patch(url: string, data = {}, config = {}) {
    return this.axiosInstance.patch(url, data, config);
  }

  delete(url: string, data?: Record<string, unknown> | null | string, config = {}) {
    return this.axiosInstance.delete(url, { data, ...config });
  }

  request(config = {}) {
    return this.axiosInstance(config);
  }
}
