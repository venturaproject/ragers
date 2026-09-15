import {
  IconChecklist,
  IconFileText,
  IconLayoutDashboard,
  IconSettings,
  IconUserCog,
  IconUsers,
  IconSparkles,
  IconPalette,
  IconBrowserCheck,
  IconNotification,
} from '@tabler/icons-react'
import { type SidebarData } from '../types'
import RagersLogo from '../ragers-logo'

export const sidebarData: SidebarData = {
  teams: [
    {
      name: 'RAG API',
      logo: RagersLogo,
      plan: 'Retrieval-Augmented Generation',
    },
  ],
  navGroups: [
    {
      title: 'General',
      items: [
        {
          title: 'Dashboard',
          url: '/admin',
          icon: IconLayoutDashboard,
        },
      ],
    },
    {
      title: 'RAG',
      items: [
        {
          title: 'Documentos',
          url: '/admin/rag/documents',
          icon: IconFileText,
          permission: 'files.process',
        },
        {
          title: 'Chat RAG',
          url: '/admin/rag/ask',
          icon: IconSparkles,
          permission: 'files.process',
        },
        {
          title: 'Procesamientos',
          url: '/admin/jobs',
          icon: IconChecklist,
          permission: 'files.batch',
        },
      ],
    },
    {
      title: 'Control de Acceso',
      items: [
        {
          // Roles/Permisos are tabs within this page (AccessControlTabs).
          title: 'Usuarios',
          url: '/admin/users',
          icon: IconUsers,
          permission: ['users.view', 'roles.view', 'permissions.view'],
        },
      ],
    },
    {
      title: 'Otros',
      items: [
        {
          title: 'Configuración',
          icon: IconSettings,
          items: [
            {
              title: 'Perfil',
              url: '/admin/settings',
              icon: IconUserCog,
            },
            {
              title: 'Apariencia',
              url: '/admin/settings/appearance',
              icon: IconPalette,
            },
            {
              title: 'Notificaciones',
              url: '/admin/settings/notifications',
              icon: IconNotification,
            },
            {
              title: 'Visualización',
              url: '/admin/settings/display',
              icon: IconBrowserCheck,
            },
          ],
        },
      ],
    },
  ],
}
