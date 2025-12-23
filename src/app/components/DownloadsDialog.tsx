import { useEffect, useState } from 'react';
import { AlertCircle, Download, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';

import { api } from '../services/api';
import type { DownloadsStatus } from '../types/api';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { ScrollArea } from './ui/scroll-area';

interface DownloadsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function DownloadsDialog({ open, onOpenChange }: DownloadsDialogProps) {
  const [status, setStatus] = useState<DownloadsStatus | null>(null);

  useEffect(() => {
    if (!open) return;
    void load();
    const interval = setInterval(() => void load(), 2000);
    return () => clearInterval(interval);
  }, [open]);

  const load = async () => {
    try {
      const s = await api.getDownloadQueue();
      setStatus(s);
    } catch (error) {
      console.error('Failed to load downloads:', error);
    }
  };

  const handleCancel = async (id: string) => {
    try {
      await api.cancelDownload(id);
      toast.success('Cancelled');
      await load();
    } catch (error) {
      toast.error('Failed to cancel');
      console.error(error);
    }
  };

  const handleRemove = async (id: string) => {
    try {
      await api.removeFromQueue(id);
      toast.success('Removed');
      await load();
    } catch (error) {
      toast.error('Failed to remove');
      console.error(error);
    }
  };

  const handleClearFailed = async () => {
    try {
      await api.clearFailedDownloads();
      toast.success('Cleared failed downloads');
      await load();
    } catch (error) {
      toast.error('Failed to clear');
      console.error(error);
    }
  };

  const items = status
    ? [status.current ? [status.current] : [], status.pending, status.failed].flat().filter(Boolean)
    : [];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh]">
        <DialogHeader>
          <DialogTitle>Downloads</DialogTitle>
          <DialogDescription>Track your model downloads</DialogDescription>
        </DialogHeader>

        <div className="flex items-center justify-between">
          <div className="text-sm text-muted-foreground">
            Max queue size: {status?.max_size ?? '—'}
          </div>
          <Button variant="outline" size="sm" onClick={() => void handleClearFailed()} disabled={!status?.failed.length}>
            <Trash2 className="size-4 mr-2" />
            Clear Failed
          </Button>
        </div>

        <ScrollArea className="h-[520px] pr-4">
          {items.length === 0 ? (
            <div className="text-center py-12 text-muted-foreground">
              <Download className="size-12 mx-auto mb-4 opacity-20" />
              <p>No downloads in progress</p>
            </div>
          ) : (
            <div className="space-y-3">
              {items.map((d) => (
                <div key={d.id} className="border rounded-lg p-4 space-y-2">
                  <div className="flex items-start justify-between gap-4">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        {d.status === 'failed' ? (
                          <AlertCircle className="size-4 text-destructive" />
                        ) : (
                          <Download className="size-4 text-muted-foreground" />
                        )}
                        <h4 className="font-medium truncate">{d.display_name}</h4>
                      </div>
                      <p className="text-sm text-muted-foreground truncate">{d.id}</p>
                      {d.error && <p className="text-sm text-destructive mt-1">{d.error}</p>}
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant={d.status === 'failed' ? 'destructive' : 'secondary'}>{d.status}</Badge>
                      {(d.status === 'queued' || d.status === 'downloading') && (
                        <Button variant="ghost" size="icon" onClick={() => void handleCancel(d.id)}>
                          <X className="size-4" />
                        </Button>
                      )}
                      {(d.status === 'failed' || d.status === 'completed' || d.status === 'cancelled') && (
                        <Button variant="ghost" size="icon" onClick={() => void handleRemove(d.id)}>
                          <Trash2 className="size-4" />
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
