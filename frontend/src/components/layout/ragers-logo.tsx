import { useState, useEffect } from 'react';
import { useTheme } from '@/context/theme-context';

const RagersLogo = ({ className = '', isCollapsed = false }) => {
  const { theme } = useTheme();

  const getLogoSrc = (currentTheme: string, collapsed: boolean) => {
    let effective = currentTheme;
    if (currentTheme === 'system') {
      effective = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    const isDark = effective === 'dark';
    return collapsed
      ? (isDark ? '/logo-ragers-compact-dark.svg' : '/logo-ragers-compact-light.svg')
      : (isDark ? '/logo-ragers-dark.svg' : '/logo-ragers-light.svg');
  };

  const [logoSrc, setLogoSrc] = useState(() => getLogoSrc(theme, isCollapsed));

  useEffect(() => {
    setLogoSrc(getLogoSrc(theme, isCollapsed));
  }, [theme, isCollapsed]);

  return (
    <img
      src={logoSrc}
      alt="Ragers"
      className={`${isCollapsed ? 'h-10 w-auto max-w-[48px]' : 'h-9 w-auto max-w-full'} ${className}`}
    />
  );
};

export default RagersLogo;
