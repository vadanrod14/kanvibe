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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Sparkles, Code } from 'lucide-react';
import type { EditorType } from 'shared/types';
import {
  EDITOR_TYPES,
  EDITOR_LABELS,
} from 'shared/types';

interface OnboardingDialogProps {
  open: boolean;
  onComplete: (config: {
    agent_market_api_key: string;
    editor: { editor_type: EditorType; custom_command: string | null };
  }) => void;
}

export function OnboardingDialog({ open, onComplete }: OnboardingDialogProps) {
  const [agentMarketApiKey, setAgentMarketApiKey] = useState<string>('');
  const [editorType, setEditorType] = useState<EditorType>('vscode');
  const [customCommand, setCustomCommand] = useState<string>('');

  const handleComplete = () => {
    onComplete({
      agent_market_api_key: agentMarketApiKey,
      editor: {
        editor_type: editorType,
        custom_command: editorType === 'custom' ? customCommand || null : null,
      },
    });
  };

  const isValid =
    agentMarketApiKey.trim() !== '' &&
    (editorType !== 'custom' ||
    (editorType === 'custom' && customCommand.trim() !== ''));

  return (
    <Dialog open={open} onOpenChange={() => {}}>
      <DialogContent className="sm:max-w-[600px]">
        <DialogHeader>
          <div className="flex items-center gap-3">
            <Sparkles className="h-6 w-6 text-primary" />
            <DialogTitle>Welcome to Vibe Kanban</DialogTitle>
          </div>
          <DialogDescription className="text-left pt-2">
            Let's set up your coding preferences. You can always change these
            later in Settings.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Sparkles className="h-4 w-4" />
                Agent Market Configuration
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="agent-market-api-key">Agent Market API Key</Label>
                <Input
                  id="agent-market-api-key"
                  type="password"
                  value={agentMarketApiKey}
                  onChange={(e) => setAgentMarketApiKey(e.target.value)}
                  placeholder="Enter your Agent Market API key"
                />
                <p className="text-sm text-muted-foreground">
                  Your API key for accessing Agent Market services. This will be used to create coding agent instances for your tasks.
                </p>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Code className="h-4 w-4" />
                Choose Your Code Editor
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="editor">Preferred Editor</Label>
                <Select
                  value={editorType}
                  onValueChange={(value: EditorType) => setEditorType(value)}
                >
                  <SelectTrigger id="editor">
                    <SelectValue placeholder="Select your preferred editor" />
                  </SelectTrigger>
                  <SelectContent>
                    {EDITOR_TYPES.map((type) => (
                      <SelectItem key={type} value={type}>
                        {EDITOR_LABELS[type]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-sm text-muted-foreground">
                  This editor will be used to open task attempts and project
                  files.
                </p>
              </div>

              {editorType === 'custom' && (
                <div className="space-y-2">
                  <Label htmlFor="custom-command">Custom Command</Label>
                  <Input
                    id="custom-command"
                    placeholder="e.g., code, subl, vim"
                    value={customCommand}
                    onChange={(e) => setCustomCommand(e.target.value)}
                  />
                  <p className="text-sm text-muted-foreground">
                    Enter the command to run your custom editor. Use spaces for
                    arguments (e.g., "code --wait").
                  </p>
                </div>
              )}
            </CardContent>
          </Card>
        </div>

        <DialogFooter>
          <Button
            onClick={handleComplete}
            disabled={!isValid}
            className="w-full"
          >
            Continue
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
