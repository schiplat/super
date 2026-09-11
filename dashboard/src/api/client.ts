import axios from 'axios';
import router from '@/router';
import { getSuperConfig } from '@/lib/superConfig';

const apiClient = axios.create({
  timeout: 10000,
  headers: {
    'Content-Type': 'application/json',
  },
});

// 1. Request interceptor: inject token
apiClient.interceptors.request.use(
  (config) => {
    const token = localStorage.getItem('super_token');
    if (token && !config.headers?.Authorization) {
      // Must be Bearer <token> to match Rust extraction logic
      config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

function isOnErrorPage(): boolean {
  return window.location.pathname.startsWith('/error/');
}

function authRequired(): boolean {
  return getSuperConfig().auth_required === true;
}

// 2. Response interceptor
apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response) {
      const status = error.response.status;

      if (status === 401) {
        localStorage.removeItem('super_token');
        // Only bounce to login when the security plugin is active.
        if (authRequired() && window.location.pathname !== '/login') {
          router.push('/login');
        }
      } else if (status === 403) {
        if (!isOnErrorPage()) router.replace('/error/403');
      } else if (status >= 500) {
        // Don't bounce while already on an error page (lets the user leave after recovery).
        if (!isOnErrorPage()) router.replace(`/error/${status}`);
      }
    } else if (error.code === 'ERR_NETWORK') {
      if (!isOnErrorPage()) router.replace('/error/503');
    }

    return Promise.reject(error);
  }
);

export default apiClient;
