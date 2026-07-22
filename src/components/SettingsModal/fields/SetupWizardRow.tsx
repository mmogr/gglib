import { FC } from 'react';
import { Button } from '../../ui/Button';
import { Label, Row } from '../../primitives';
import { updateSettings } from '../../../services/transport/api/settings';

interface SetupWizardRowProps {
  saving: boolean;
}

/**
 * Lets the user re-trigger the first-run setup wizard.
 */
export const SetupWizardRow: FC<SetupWizardRowProps> = ({ saving }) => (
  <Row justify="between" gap="sm" className="items-center">
    <div>
      <Label>Setup Wizard</Label>
      <p className="text-text-secondary text-sm mt-1">Re-run the first-run system configuration wizard</p>
    </div>
    <Button
      type="button"
      variant="outline"
      size="sm"
      onClick={() => {
        updateSettings({ setupCompleted: false }).then(() => {
          window.location.reload();
        });
      }}
      disabled={saving}
    >
      Re-run Wizard
    </Button>
  </Row>
);
