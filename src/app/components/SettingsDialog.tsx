import { useEffect, useMemo, useState } from 'react';
import { Check, Copy, Play, Square } from 'lucide-react';
import { toast } from 'sonner';

import { api } from '../services/api';
import type { ProxyState, Settings } from '../types/api';
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
import { Label } from './ui/label';
import { Switch } from './ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const [proxyStatus, setProxyStatus] = useState<ProxyState | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [proxyPort, setProxyPort] = useState<number>(8080);
  const [defaultContext, setDefaultContext] = useState<number>(4096);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    void load();
  }, [open]);

  const load = async () => {
    try {
      const [p, s] = await Promise.all([api.getProxyStatus(), api.getSettings()]);
      setProxyStatus(p);
      setSettings(s);
      setProxyPort(s.proxy_port ?? p.port ?? 8080);
      setDefaultContext(s.default_context_size ?? 4096);
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  };

  const isProxyRunning = proxyStatus?.running ?? false;
  const endpoint = useMemo(() => `http://127.0.0.1:${proxyStatus?.port ?? proxyPort}/v1`, [proxyStatus?.port, proxyPort]);

  const handleStartProxy = async () => {
    setLoading(true);
    try {
      const res = await api.startProxy({ port: proxyPort, default_context: defaultContext });
      setProxyStatus(res);
      await api.updateSettings({ proxy_port: proxyPort, default_context_size: defaultContext });
      toast.success('Proxy started');
    } catch (error) {
      toast.error('Failed to start proxy');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const handleStopProxy = async () => {
    setLoading(true);
    try {
      await api.stopProxy();
      toast.success('Proxy stopped');
      await load();
    } catch (error) {
      toast.error('Failed to stop proxy');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const handleCopyUrl = () => {
    void navigator.clipboard.writeText(endpoint);
    setCopied(true);
    toast.success('URL copied to clipboard');
    setTimeout(() => setCopied(false), 2000);
  };

  const handleToggleMemoryFit = async (value: boolean) => {
    try {
      const updated = await api.updateSettings({ show_memory_fit_indicators: value });
      setSettings(updated);
    } catch (error) {
      toast.error('Failed to update setting');
      console.error(error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Configure GGLib</DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="proxy" className="mt-4">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="proxy">OpenAI Proxy</TabsTrigger>
            <TabsTrigger value="general">General</TabsTrigger>
          </TabsList>

          <TabsContent value="proxy" className="space-y-4 mt-4">
            <div className="flex items-center justify-between p-4 border rounded-lg">
              <div>
                <div className="flex items-center gap-2 mb-1">
                  <h4 className="font-medium">Proxy Server</h4>
                  <Badge variant={isProxyRunning ? 'default' : 'secondary'}>
                    {isProxyRunning ? 'Running' : 'Stopped'}
                  </Badge>
                </div>
                <p className="text-sm text-muted-foreground">OpenAI-compatible API proxy</p>
              </div>
              <div className="flex gap-2">
                {isProxyRunning ? (
                  <Button variant="destructive" onClick={() => void handleStopProxy()} disabled={loading}>
                    <Square className="size-4 mr-2" />
                    Stop
                  </Button>
                ) : (
                  <Button onClick={() => void handleStartProxy()} disabled={loading}>
                    <Play className="size-4 mr-2" />
                    Start
                  </Button>
                )}
              </div>
            </div>

            <div className="space-y-4 p-4 border rounded-lg">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="proxy-port">Port</Label>
                  <Input
                    id="proxy-port"
                    type="number"
                    value={proxyPort}
                    onChange={(e) => setProxyPort(Number.parseInt(e.target.value || '0', 10))}
                    disabled={isProxyRunning}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="default-context">Default Context Size</Label>
                  <Input
                    id="default-context"
                    type="number"
                    value={defaultContext}
                    onChange={(e) => setDefaultContext(Number.parseInt(e.target.value || '0', 10))}
                    disabled={isProxyRunning}
                  />
                </div>
              </div>

              {isProxyRunning && (
                <div className="space-y-2">
                  <Label>API Endpoint</Label>
                  <div className="flex gap-2">
                    <Input value={endpoint} readOnly />
                    <Button variant="outline" size="icon" onClick={handleCopyUrl}>
                      {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">Use this URL in OpenWebUI or any OpenAI-compatible client</p>
                </div>
              )}
            </div>
          </TabsContent>

          <TabsContent value="general" className="space-y-4 mt-4">
            <div className="flex items-center justify-between p-4 border rounded-lg">
              <div>
                <h4 className="font-medium mb-1">Memory Fit Indicators</h4>
                <p className="text-sm text-muted-foreground">Show “will it fit?” hints</p>
              </div>
              <Switch
                checked={settings?.show_memory_fit_indicators ?? false}
                onCheckedChange={(v) => void handleToggleMemoryFit(v)}
              />
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
