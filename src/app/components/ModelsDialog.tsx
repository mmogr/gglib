import { useState } from 'react';
import { FolderOpen, Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';

import type { Model } from '../types/api';
import { api } from '../services/api';
import { pickGgufFile } from '../../services/platform/fileDialogs';
import { Badge } from './ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from './ui/table';

interface ModelsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  models: Model[];
  onModelsChange: () => void | Promise<void>;
}

export function ModelsDialog({ open, onOpenChange, models, onModelsChange }: ModelsDialogProps) {
  const [newModelPath, setNewModelPath] = useState('');
  const [adding, setAdding] = useState(false);

  const handlePickFile = async () => {
    const res = await pickGgufFile();
    if (!res.cancelled && res.path) {
      setNewModelPath(res.path);
    }
  };

  const handleAddModel = async () => {
    if (!newModelPath.trim()) return;
    setAdding(true);
    try {
      await api.addModelPath(newModelPath.trim());
      toast.success('Model added successfully');
      setNewModelPath('');
      await onModelsChange();
    } catch (error) {
      toast.error('Failed to add model');
      console.error(error);
    } finally {
      setAdding(false);
    }
  };

  const handleRemoveModel = async (id: number | undefined) => {
    if (!id) return;
    if (!confirm('Are you sure you want to remove this model?')) return;

    try {
      await api.removeModel(id);
      toast.success('Model removed');
      await onModelsChange();
    } catch (error) {
      toast.error('Failed to remove model');
      console.error(error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[80vh]">
        <DialogHeader>
          <DialogTitle>Manage Models</DialogTitle>
          <DialogDescription>Add or remove GGUF models from your library</DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="flex gap-2">
            <div className="flex-1 space-y-2">
              <Label htmlFor="model-path">Model Path</Label>
              <Input
                id="model-path"
                value={newModelPath}
                onChange={(e) => setNewModelPath(e.target.value)}
                placeholder="/path/to/model.gguf"
              />
            </div>
            <div className="flex items-end gap-2">
              <Button variant="outline" size="icon" onClick={() => void handlePickFile()}>
                <FolderOpen className="size-4" />
              </Button>
              <Button onClick={() => void handleAddModel()} disabled={adding}>
                <Plus className="size-4 mr-2" />
                Add Model
              </Button>
            </div>
          </div>

          <div className="border rounded-lg">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Quantization</TableHead>
                  <TableHead>Params</TableHead>
                  <TableHead>Path</TableHead>
                  <TableHead className="w-[100px]">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {models.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center text-muted-foreground">
                      No models found. Add one to get started.
                    </TableCell>
                  </TableRow>
                ) : (
                  models.map((model) => (
                    <TableRow key={model.id ?? model.file_path}>
                      <TableCell className="font-medium">{model.name}</TableCell>
                      <TableCell>
                        {model.quantization && <Badge variant="outline">{model.quantization}</Badge>}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {model.param_count_b ? `${model.param_count_b}B` : '—'}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground truncate max-w-[260px]">
                        {model.file_path}
                      </TableCell>
                      <TableCell>
                        <div className="flex gap-1">
                          <Button
                            variant="ghost"
                            size="icon"
                            disabled={!model.id}
                            onClick={() => void handleRemoveModel(model.id)}
                          >
                            <Trash2 className="size-4" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
