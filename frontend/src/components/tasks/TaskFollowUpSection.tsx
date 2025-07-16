import { AlertCircle, Send } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { FileSearchTextarea } from '@/components/ui/file-search-textarea';
import { useContext, useMemo, useState } from 'react';
import { attemptsApi, ApiError } from '@/lib/api.ts';
import {
  TaskAttemptDataContext,
  TaskDetailsContext,
  TaskSelectedAttemptContext,
} from '@/components/context/taskDetailsContext.ts';
import { Loader } from '@/components/ui/loader';
import { AgentMarketApiKeyDialog } from '@/components/AgentMarketApiKeyDialog';

export function TaskFollowUpSection() {
  const { task, projectId } = useContext(TaskDetailsContext);
  const { selectedAttempt } = useContext(TaskSelectedAttemptContext);
  const { attemptData, fetchAttemptData, isAttemptRunning } = useContext(
    TaskAttemptDataContext
  );

  const [followUpMessage, setFollowUpMessage] = useState('');
  const [isSendingFollowUp, setIsSendingFollowUp] = useState(false);
  const [followUpError, setFollowUpError] = useState<string | null>(null);

  // Add state for API key dialog
  const [showApiKeyDialog, setShowApiKeyDialog] = useState(false);
  const [pendingFollowUpMessage, setPendingFollowUpMessage] = useState<string>('');

  const canSendFollowUp = useMemo(() => {
    if (
      !selectedAttempt ||
      attemptData.activities.length === 0 ||
      isAttemptRunning ||
      isSendingFollowUp
    ) {
      return false;
    }

    const codingAgentActivities = attemptData.activities.filter(
      (activity) => activity.status === 'executorcomplete'
    );

    return codingAgentActivities.length > 0;
  }, [
    selectedAttempt,
    attemptData.activities,
    isAttemptRunning,
    isSendingFollowUp,
  ]);

  const onSendFollowUp = async () => {
    if (!task || !selectedAttempt || !followUpMessage.trim()) return;

    try {
      setIsSendingFollowUp(true);
      setFollowUpError(null);
      await attemptsApi.followUp(
        projectId!,
        selectedAttempt.task_id,
        selectedAttempt.id,
        {
          prompt: followUpMessage.trim(),
        }
      );
      setFollowUpMessage('');
      fetchAttemptData(selectedAttempt.id, selectedAttempt.task_id);
    } catch (error: unknown) {
      // Check if this is the specific Agent Market API key error
      if (error instanceof ApiError && 
          (error as ApiError).message === 'Agent Market API key is required to run tasks. Please configure your API key in the application settings.') {
        // Store the follow-up message for retry after API key is configured
        setPendingFollowUpMessage(followUpMessage.trim());
        setShowApiKeyDialog(true);
      } else {
        // @ts-expect-error it is type ApiError
        setFollowUpError(`Failed to start follow-up execution: ${error.message}`);
      }
    } finally {
      setIsSendingFollowUp(false);
    }
  };

  const handleApiKeySaved = async () => {
    setShowApiKeyDialog(false);
    
    // Retry sending the follow-up with the stored message
    if (pendingFollowUpMessage && task && selectedAttempt) {
      const messageToSend = pendingFollowUpMessage;
      setPendingFollowUpMessage('');
      
      try {
        setIsSendingFollowUp(true);
        setFollowUpError(null);
        await attemptsApi.followUp(
          projectId!,
          selectedAttempt.task_id,
          selectedAttempt.id,
          {
            prompt: messageToSend,
          }
        );
        setFollowUpMessage('');
        fetchAttemptData(selectedAttempt.id, selectedAttempt.task_id);
      } catch (error: unknown) {
        console.error('Failed to start follow-up execution after API key configuration:', error);
        // @ts-expect-error it is type ApiError
        setFollowUpError(`Failed to start follow-up execution: ${error.message}`);
      } finally {
        setIsSendingFollowUp(false);
      }
    }
  };

  return (
    selectedAttempt && (
      <div className="border-t p-4">
        <div className="space-y-2">
          {followUpError && (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>{followUpError}</AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <h4 className="text-sm font-medium">Follow-up Instructions</h4>
              {isSendingFollowUp && <Loader size={16} />}
            </div>
            <FileSearchTextarea
              projectId={projectId!}
              value={followUpMessage}
              onChange={setFollowUpMessage}
              placeholder="Add follow-up instructions for the coding agent..."
              disabled={!canSendFollowUp || isSendingFollowUp}
              className="min-h-[80px]"
            />
            <Button
              onClick={onSendFollowUp}
              disabled={
                !canSendFollowUp ||
                !followUpMessage.trim() ||
                isSendingFollowUp
              }
              size="sm"
              className="w-full"
            >
              <Send className="h-4 w-4 mr-2" />
              {isSendingFollowUp ? 'Sending...' : 'Send Follow-up'}
            </Button>
          </div>
        </div>

        <AgentMarketApiKeyDialog
          open={showApiKeyDialog}
          onOpenChange={setShowApiKeyDialog}
          onApiKeySaved={handleApiKeySaved}
        />
      </div>
    )
  );
}
