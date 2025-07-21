import { Dispatch, SetStateAction, useContext, useMemo, useState } from 'react';
import { Button } from '@/components/ui/button.tsx';
import {
  ArrowDown,
  GitBranch as GitBranchIcon,
  Play,
  Search,
  X,
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu.tsx';
import { Input } from '@/components/ui/input.tsx';
import type { GitBranch, TaskAttempt } from 'shared/types.ts';
import { attemptsApi, ApiError } from '@/lib/api.ts';
import {
  TaskAttemptDataContext,
  TaskDetailsContext,
} from '@/components/context/taskDetailsContext.ts';
import { AgentMarketApiKeyDialog } from '@/components/AgentMarketApiKeyDialog';

type Props = {
  branches: GitBranch[];
  taskAttempts: TaskAttempt[];
  createAttemptExecutor: string;
  createAttemptBranch: string | null;
  selectedExecutor: string;
  selectedBranch: string | null;
  fetchTaskAttempts: () => void;
  setIsInCreateAttemptMode: Dispatch<SetStateAction<boolean>>;
  setCreateAttemptBranch: Dispatch<SetStateAction<string | null>>;
  setCreateAttemptExecutor: Dispatch<SetStateAction<string>>;
  availableExecutors: {
    id: string;
    name: string;
  }[];
};

function CreateAttempt({
  branches,
  taskAttempts,
  createAttemptExecutor: _createAttemptExecutor,
  createAttemptBranch,
  selectedExecutor: _selectedExecutor,
  selectedBranch,
  fetchTaskAttempts,
  setIsInCreateAttemptMode,
  setCreateAttemptBranch,
  setCreateAttemptExecutor: _setCreateAttemptExecutor,
  availableExecutors: _availableExecutors,
}: Props) {
  const { task, projectId } = useContext(TaskDetailsContext);
  const { isAttemptRunning } = useContext(TaskAttemptDataContext);

  const [branchSearchTerm, setBranchSearchTerm] = useState('');
  const [showApiKeyDialog, setShowApiKeyDialog] = useState(false);
  const [pendingAttemptParams, setPendingAttemptParams] = useState<{
    executor?: string;
    baseBranch?: string;
  } | null>(null);

  // Filter branches based on search term
  const filteredBranches = useMemo(() => {
    if (!branchSearchTerm.trim()) {
      return branches;
    }
    return branches.filter((branch) =>
      branch.name.toLowerCase().includes(branchSearchTerm.toLowerCase())
    );
  }, [branches, branchSearchTerm]);

  const onCreateNewAttempt = async (baseBranch?: string) => {
    try {
      await attemptsApi.create(projectId!, task.id, {
        executor: 'agent_market',
        base_branch: baseBranch || selectedBranch,
      });
      fetchTaskAttempts();
    } catch (error) {
      // Check if this is the specific Agent Market API key error
      if (error instanceof ApiError && 
          error.message === 'Agent Market API key is required to run tasks. Please configure your API key in the application settings.') {
        // Store the attempt parameters for retry after API key is configured
        setPendingAttemptParams({ baseBranch });
        setShowApiKeyDialog(true);
      } else if (error instanceof ApiError) {
        // For other API errors, show the specific error message to the user
        // This will now catch Agent Market authentication errors and other API failures
        console.error('Failed to create task attempt:', error.message);
        // You might want to add a toast notification or state to show this error
        alert(`Failed to create task attempt: ${error.message}`);
      } else {
        // For unexpected errors, show a generic error message
        console.error('Failed to create task attempt:', error);
        alert('An unexpected error occurred while creating the task attempt. Please try again.');
      }
    }
  };

  const handleApiKeySaved = async () => {
    setShowApiKeyDialog(false);
    
    // Retry creating the attempt with the stored parameters
    if (pendingAttemptParams) {
      const { baseBranch } = pendingAttemptParams;
      setPendingAttemptParams(null);
      
      try {
        await attemptsApi.create(projectId!, task.id, {
          executor: 'agent_market',
          base_branch: baseBranch || selectedBranch,
        });
        fetchTaskAttempts();
      } catch (error) {
        console.error('Failed to create task attempt after API key configuration:', error);
        if (error instanceof ApiError) {
          alert(`Failed to create task attempt: ${error.message}`);
        } else {
          alert('An unexpected error occurred while creating the task attempt. Please try again.');
        }
      }
    }
  };

  const handleExitCreateAttemptMode = () => {
    setIsInCreateAttemptMode(false);
  };

  const handleCreateAttempt = () => {
    onCreateNewAttempt(createAttemptBranch || undefined);
    handleExitCreateAttemptMode();
  };

  return (
    <>
      <div className="p-4 bg-muted/20 rounded-lg border">
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-base font-semibold">Create Attempt</h3>
            {taskAttempts.length > 0 && (
              <Button
                variant="ghost"
                size="sm"
                onClick={handleExitCreateAttemptMode}
              >
                <X className="h-4 w-4" />
              </Button>
            )}
          </div>
          <div className="flex items-center w-4/5">
            <label className="text-xs font-medium text-muted-foreground">
              Each time you start an attempt, a new session is initiated with the
              Agent Market coding agent, and a git worktree and corresponding task
              branch are created.
            </label>
          </div>

          <div className="grid grid-cols-2 gap-3 items-end">
            {/* Step 1: Choose Base Branch */}
            <div className="space-y-1">
              <div className="flex items-center gap-1.5">
                <label className="text-xs font-medium text-muted-foreground">
                  Base branch
                </label>
              </div>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full justify-between text-xs"
                  >
                    <div className="flex items-center gap-1.5">
                      <GitBranchIcon className="h-3 w-3" />
                      <span className="truncate">
                        {createAttemptBranch
                          ? createAttemptBranch.includes('/')
                            ? createAttemptBranch.split('/').pop()
                            : createAttemptBranch
                          : 'current'}
                      </span>
                    </div>
                    <ArrowDown className="h-3 w-3" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent className="w-80">
                  <div className="p-2">
                    <div className="relative">
                      <Search className="absolute left-2 top-2.5 h-4 w-4 text-muted-foreground" />
                      <Input
                        placeholder="Search branches..."
                        value={branchSearchTerm}
                        onChange={(e) => setBranchSearchTerm(e.target.value)}
                        className="pl-8"
                      />
                    </div>
                  </div>
                  <DropdownMenuSeparator />
                  <div className="max-h-64 overflow-y-auto">
                    {filteredBranches.length === 0 ? (
                      <div className="p-2 text-sm text-muted-foreground text-center">
                        No branches found
                      </div>
                    ) : (
                      filteredBranches.map((branch) => (
                        <DropdownMenuItem
                          key={branch.name}
                          onClick={() => {
                            setCreateAttemptBranch(branch.name);
                            setBranchSearchTerm('');
                          }}
                          className={
                            createAttemptBranch === branch.name ? 'bg-accent' : ''
                          }
                        >
                          <div className="flex items-center justify-between w-full">
                            <span
                              className={branch.is_current ? 'font-medium' : ''}
                            >
                              {branch.name}
                            </span>
                            <div className="flex gap-1">
                              {branch.is_current && (
                                <span className="text-xs bg-green-100 text-green-800 px-1 rounded">
                                  current
                                </span>
                              )}
                              {branch.is_remote && (
                                <span className="text-xs bg-blue-100 text-blue-800 px-1 rounded">
                                  remote
                                </span>
                              )}
                            </div>
                          </div>
                        </DropdownMenuItem>
                      ))
                    )}
                  </div>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            {/* Step 2: Start Attempt */}
            <div className="space-y-1">
              <Button
                onClick={handleCreateAttempt}
                disabled={isAttemptRunning}
                size="sm"
                className="w-full text-xs"
              >
                <Play className="h-3 w-3 mr-1.5" />
                Start
              </Button>
            </div>
          </div>
        </div>
      </div>

      <AgentMarketApiKeyDialog
        open={showApiKeyDialog}
        onOpenChange={setShowApiKeyDialog}
        onApiKeySaved={handleApiKeySaved}
      />
    </>
  );
}

export default CreateAttempt;
