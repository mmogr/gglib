import { useEffect, useState } from 'react';
import { Download, Search, Star } from 'lucide-react';
import { toast } from 'sonner';

import type { HuggingFaceModel } from '../types/api';
import { api } from '../services/api';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Input } from './ui/input';
import { ScrollArea } from './ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';

interface HuggingFaceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function HuggingFaceDialog({ open, onOpenChange }: HuggingFaceDialogProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<HuggingFaceModel[]>([]);
  const [popularModels, setPopularModels] = useState<HuggingFaceModel[]>([]);
  const [searching, setSearching] = useState(false);
  const [selectedModel, setSelectedModel] = useState<HuggingFaceModel | null>(null);
  const [quantizations, setQuantizations] = useState<{ name: string; size_mb: number }[]>([]);

  useEffect(() => {
    if (!open) return;
    void loadPopularModels();
  }, [open]);

  const loadPopularModels = async () => {
    try {
      const resp = await api.browseHfModels({ page: 0, limit: 20, sort_by: 'downloads', sort_ascending: false });
      setPopularModels(resp.models);
    } catch (error) {
      console.error('Failed to load popular models:', error);
    }
  };

  const handleSearch = async () => {
    if (!searchQuery.trim()) return;

    setSearching(true);
    try {
      const resp = await api.browseHfModels({ page: 0, limit: 30, query: searchQuery.trim(), sort_by: 'downloads', sort_ascending: false });
      setSearchResults(resp.models);
    } catch (error) {
      toast.error('Search failed');
      console.error(error);
    } finally {
      setSearching(false);
    }
  };

  const handleSelectModel = async (model: HuggingFaceModel) => {
    setSelectedModel(model);
    setQuantizations([]);
    try {
      const q = await api.getHfQuantizations(model.id);
      setQuantizations(q.quantizations.map((x) => ({ name: x.name, size_mb: x.size_mb })));
    } catch (error) {
      toast.error('Failed to load quantizations');
      console.error(error);
    }
  };

  const handleDownload = async (model: HuggingFaceModel, quantization: string) => {
    try {
      const settings = await api.getSettings();
      await api.queueDownload({ modelId: model.id, quantization, targetPath: settings.default_download_path ?? undefined });
      toast.success('Download queued');
    } catch (error) {
      toast.error('Failed to queue download');
      console.error(error);
    }
  };

  const formatNumber = (num?: number) => {
    if (!num) return '0';
    if (num < 1000) return num.toString();
    if (num < 1000000) return `${(num / 1000).toFixed(1)}K`;
    return `${(num / 1000000).toFixed(1)}M`;
  };

  const ModelCard = ({ model }: { model: HuggingFaceModel }) => (
    <div
      className="border rounded-lg p-4 hover:bg-accent transition-colors cursor-pointer"
      onClick={() => void handleSelectModel(model)}
    >
      <div className="flex items-start justify-between gap-4 mb-3">
        <div className="flex-1 min-w-0">
          <h4 className="font-medium truncate mb-1">{model.id}</h4>
          {model.author && <p className="text-sm text-muted-foreground">by {model.author}</p>}
        </div>
        <div className="flex items-center gap-3 text-sm text-muted-foreground">
          <div className="flex items-center gap-1">
            <Download className="size-3" />
            {formatNumber(model.downloads)}
          </div>
          <div className="flex items-center gap-1">
            <Star className="size-3" />
            {formatNumber(model.likes)}
          </div>
        </div>
      </div>

      {model.tags && model.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {model.tags.slice(0, 5).map((tag) => (
            <Badge key={tag} variant="secondary" className="text-xs">
              {tag}
            </Badge>
          ))}
        </div>
      )}
    </div>
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[80vh]">
        <DialogHeader>
          <DialogTitle>HuggingFace Hub</DialogTitle>
          <DialogDescription>Browse and download GGUF models from HuggingFace</DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="browse" className="flex-1">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="browse">Browse</TabsTrigger>
            <TabsTrigger value="search">Search</TabsTrigger>
          </TabsList>

          <TabsContent value="browse" className="mt-4">
            <ScrollArea className="h-[500px] pr-4">
              <div className="space-y-3">
                {popularModels.map((model) => (
                  <ModelCard key={model.id} model={model} />
                ))}
              </div>
            </ScrollArea>
          </TabsContent>

          <TabsContent value="search" className="mt-4 space-y-4">
            <div className="flex gap-2">
              <Input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && void handleSearch()}
                placeholder="Search for GGUF models..."
              />
              <Button onClick={() => void handleSearch()} disabled={searching}>
                <Search className="size-4 mr-2" />
                Search
              </Button>
            </div>

            <ScrollArea className="h-[450px] pr-4">
              {searchResults.length === 0 ? (
                <div className="text-center py-12 text-muted-foreground">
                  <Search className="size-12 mx-auto mb-4 opacity-20" />
                  <p>No results yet. Try searching for a model.</p>
                </div>
              ) : (
                <div className="space-y-3">
                  {searchResults.map((model) => (
                    <ModelCard key={model.id} model={model} />
                  ))}
                </div>
              )}
            </ScrollArea>
          </TabsContent>
        </Tabs>

        {selectedModel && (
          <div className="border-t pt-4 mt-4">
            <h3 className="font-medium mb-2">Quantizations</h3>
            <ScrollArea className="h-40">
              <div className="space-y-2">
                {quantizations.length === 0 ? (
                  <div className="text-sm text-muted-foreground">No quantizations loaded.</div>
                ) : (
                  quantizations.map((q) => (
                    <div key={q.name} className="flex items-center justify-between p-2 rounded border">
                      <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium truncate">{q.name}</p>
                        <p className="text-xs text-muted-foreground">{q.size_mb.toFixed(0)} MB</p>
                      </div>
                      <Button size="sm" onClick={() => void handleDownload(selectedModel, q.name)}>
                        <Download className="size-4 mr-2" />
                        Download
                      </Button>
                    </div>
                  ))
                )}
              </div>
            </ScrollArea>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
