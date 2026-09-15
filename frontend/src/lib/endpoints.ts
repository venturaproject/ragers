// Central API endpoint registry — change the base path here to swap backends
const API_BASE = '/api/v1'

export const endpoints = {
  config: `${API_BASE}/config`,
  audit: `${API_BASE}/audit`,
  auth: {
    login:                    `${API_BASE}/auth/login`,
    logout:                   `${API_BASE}/auth/logout`,
    refresh:                  `${API_BASE}/auth/refresh`,
    me:                       `${API_BASE}/auth/me`,
    avatar:                   `${API_BASE}/auth/me/avatar`,
    forgotPassword:           `${API_BASE}/auth/forgot-password`,
    resetPassword:            `${API_BASE}/auth/reset-password`,
    verificationNotification: `${API_BASE}/auth/email/verification-notification`,
    csrf:                     `${API_BASE}/csrf/`,
  },
  users: {
    list:   `${API_BASE}/users`,
    detail: (id: string | number) => `${API_BASE}/users/${id}`,
  },
  roles: {
    list:   `${API_BASE}/roles`,
    detail: (id: string | number) => `${API_BASE}/roles/${id}`,
  },
  permissions: {
    list:   `${API_BASE}/permissions`,
    detail: (id: string | number) => `${API_BASE}/permissions/${id}`,
  },
  dashboard: `${API_BASE}/dashboard/`,
  notifications: {
    list:        `${API_BASE}/notifications`,
    detail:      (id: string | number) => `${API_BASE}/notifications/${id}`,
    markRead:    (id: string | number) => `${API_BASE}/notifications/${id}/read`,
    markAllRead: `${API_BASE}/notifications/read-all`,
  },
  upload: {
    presignedUrl: `${API_BASE}/upload/presigned-url`,
  },
  settings: {
    appearance:    `${API_BASE}/settings/appearance`,
    notifications: `${API_BASE}/settings/notifications`,
    display:       `${API_BASE}/settings/display`,
  },
  apiClients: {
    list:   `${API_BASE}/api-clients`,
    create: `${API_BASE}/api-clients`,
    update: (id: string) => `${API_BASE}/api-clients/${id}`,
    usage:  (id: string) => `${API_BASE}/api-clients/${id}/usage`,
    rotate: (id: string) => `${API_BASE}/api-clients/${id}/rotate`,
    revoke: (id: string) => `${API_BASE}/api-clients/${id}`,
  },
  jobs: {
    list:   `${API_BASE}/jobs`,
    detail: (id: string) => `${API_BASE}/jobs/${id}`,
  },
  rag: {
    documents:       `/api/rag/documents`,
    documentDetail:  (id: string) => `/api/rag/documents/${id}`,
    query:           `/api/rag/query`,
    syncErp:         `/api/rag/sync/erp`,
  },
  chat: {
    threads:       `/api/chat/threads`,
    threadDetail:  (id: string) => `/api/chat/threads/${id}`,
    messages:      (id: string) => `/api/chat/threads/${id}/messages`,
  },
}
