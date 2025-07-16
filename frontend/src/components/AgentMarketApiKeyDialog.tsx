import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useConfig } from '@/components/config-provider';
import { Alert, AlertDescription } from '@/components/ui/alert';

interface AgentMarketApiKeyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onApiKeySaved: () => void;
}

export function AgentMarketApiKeyDialog({
  open,
  onOpenChange,
  onApiKeySaved,
}: AgentMarketApiKeyDialogProps) {
  const { config, updateAndSaveConfig } = useConfig();
  const [apiKey, setApiKey] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!apiKey.trim()) {
      return;
    }

    if (!config) {
      setError('Configuration not loaded');
      return;
    }

    setIsLoading(true);
    setError(null);
    
    try {
      // Save the Agent Market API key using the config API
      await updateAndSaveConfig({
        agent_market_api_key: apiKey.trim(),
      });
      
      // If we reach here, the save was successful
      onApiKeySaved();
      setApiKey('');
      onOpenChange(false);
    } catch (error) {
      console.error('Failed to save Agent Market API key:', error);
      setError('Failed to save API key. Please try again.');
    } finally {
      setIsLoading(false);
    }
  };

  const handleCancel = () => {
    setApiKey('');
    setError(null);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Configure Agent Market API Key</DialogTitle>
          <DialogDescription>
            An Agent Market API key is required to run tasks. Please enter your API key to continue.
          </DialogDescription>
        </DialogHeader>
        
        <div className="grid gap-4 py-4">
          <div className="grid grid-cols-4 items-center gap-4">
            <Label htmlFor="api-key" className="text-right">
              API Key
            </Label>
            <Input
              id="api-key"
              type="password"
              placeholder="Enter your Agent Market API key"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              className="col-span-3"
            />
          </div>
          
          {error && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
        </div>
        
        <DialogFooter>
          <Button variant="outline" onClick={handleCancel} disabled={isLoading}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!apiKey.trim() || isLoading}>
            {isLoading ? 'Saving...' : 'Save API Key'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
