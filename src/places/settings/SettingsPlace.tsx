/**
 * The settings place, at `/settings/:section`: six sections on the left,
 * the open one on the right, and every field saving as it changes, with no
 * Save and no Cancel. A segment that names no section goes to General.
 *
 * Opening the place reads the settings again, as the provider holds them,
 * since a terminal may have written any of them since.
 *
 * @module places/settings/SettingsPlace
 */

import { useEffect, type FC } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router';
import { paths, settingsSection, SETTINGS_SECTIONS, type SettingsSection } from '../../app/paths';
import { StatusMark } from '../../components/status/StatusMark';
import { Button } from '../../components/ui/Button';
import { useSettings } from '../../hooks/useSettings';
import { cn } from '../../utils/cn';
import { AboutSection } from './AboutSection';
import { EndpointSection } from './EndpointSection';
import { EnginesSection } from './EnginesSection';
import { GeneralSection } from './GeneralSection';
import { ModelsSection } from './ModelsSection';
import { ToolsSection } from './ToolsSection';

/** Each section's name in the list and over its content. */
export const SECTION_NAMES: Readonly<Record<SettingsSection, string>> = {
  general: 'General',
  endpoint: 'Endpoint',
  models: 'Models',
  tools: 'Tools',
  engines: 'Engines',
  about: 'About',
};

const CONTENT: Readonly<Record<SettingsSection, FC>> = {
  general: GeneralSection,
  endpoint: EndpointSection,
  models: ModelsSection,
  tools: ToolsSection,
  engines: EnginesSection,
  about: AboutSection,
};

/** The sections that hold settings, which wait for the settings to load. */
const READS_SETTINGS: ReadonlySet<SettingsSection> = new Set(['general', 'endpoint', 'models', 'tools']);

export default function SettingsPlace() {
  const section = settingsSection(useParams().section);
  const navigate = useNavigate();
  const { settings, error, refresh } = useSettings();

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (section === null) return <Navigate to={paths.settings()} replace />;
  const Content = CONTENT[section];
  const waiting = READS_SETTINGS.has(section) && settings === null;

  return (
    <section aria-label="Settings" className="flex min-w-0 flex-1 flex-col overflow-hidden wide:flex-row">
      <nav aria-label="Settings sections" className="flex shrink-0 flex-wrap gap-xs px-sm pt-md wide:w-[176px] wide:flex-col wide:flex-nowrap">
        {SETTINGS_SECTIONS.map((id) => (
          <Button
            key={id}
            variant="ghost"
            size="sm"
            className={cn('justify-start text-sm font-normal text-text', id === section && 'bg-surface-hover')}
            aria-current={id === section ? 'page' : undefined}
            onClick={() => navigate(paths.settings(id))}
          >
            {SECTION_NAMES[id]}
          </Button>
        ))}
      </nav>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <header className="shrink-0 px-md pb-sm pt-md">
          <h2 className="m-0 text-base font-semibold text-text">{SECTION_NAMES[section]}</h2>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto pb-md">
          <div className="flex max-w-[720px] flex-col gap-lg px-md">
            {waiting ? (
              error !== null ? (
                <p role="alert" className="m-0 text-sm text-text-secondary">
                  {`Couldn't read the settings: ${error}`}
                </p>
              ) : (
                <StatusMark kind="changing" label="Asking gglib" />
              )
            ) : (
              <Content key={section} />
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
