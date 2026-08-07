import { useState, FC, FormEvent } from "react";
import { FolderOpen } from "lucide-react";
import { appLogger } from '../services/platform';
import { pickGgufFile, isDesktop } from "../services/platform";
import { Button } from "./ui/Button";
import { Banner } from './ui/Banner';
import { Icon } from "./ui/Icon";
import { Input } from "./ui/Input";
import { getTransport } from '../services/transport';

interface AddModelProps {
  onModelAdded: () => void;
}

const AddModel: FC<AddModelProps> = ({ onModelAdded }) => {
  const [filePath, setFilePath] = useState("");
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleBrowse = async () => {
    try {
      const result = await pickGgufFile();
      if (!result.cancelled && result.path) {
        setFilePath(result.path);
        if (!isDesktop()) {
          setError("Note: In web mode, you'll need to provide the full server path");
        }
      }
    } catch (err) {
      appLogger.error('component.model', 'Failed to open file dialog', { error: err });
      setError("File browser not available. Please enter the path manually.");
    }
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!filePath.trim()) {
      setError("Please provide a file path");
      return;
    }

    try {
      setAdding(true);
      setError(null);
      await getTransport().addModel({ filePath: filePath.trim() });
      setFilePath("");
      onModelAdded();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add model");
    } finally {
      setAdding(false);
    }
  };

  return (
    <div className="bg-surface rounded-lg p-xl max-w-[800px] shadow-md border border-border">
      <h2>Add New Model</h2>

      <form onSubmit={handleSubmit} className="add-model-form">
        <div className="mb-lg">
          <label htmlFor="filePath">Model File Path:</label>
          <div className="flex flex-col items-stretch gap-sm md:flex-row md:flex-wrap">
            <Input
              type="text"
              id="filePath"
              value={filePath}
              onChange={(e) => setFilePath(e.target.value)}
              placeholder="/path/to/your/model.gguf"
              className="flex-[1_1_200px] min-w-0"
            />
            <Button
              type="button"
              onClick={handleBrowse}
              leftIcon={<Icon icon={FolderOpen} size={14} />}
              className="w-full md:w-auto"
            >
              Browse
            </Button>
          </div>
        </div>

        {error && <Banner variant="danger">{error}</Banner>}

        <div className="form-actions">
          <Button
            type="submit"
            disabled={adding || !filePath.trim()}
            variant="primary"
          >
            {adding ? "Adding Model..." : "Add Model"}
          </Button>
        </div>
      </form>
    </div>
  );
};

export default AddModel;