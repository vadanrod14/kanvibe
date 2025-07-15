import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { FileSearchTextarea } from '@/components/ui/file-search-textarea';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { TaskStatus } from 'shared/types';

interface Task {
  id: string;
  project_id: string;
  title: string;
  description: string | null;
  status: TaskStatus;
  created_at: string;
  updated_at: string;
}

interface TaskFormDialogProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  task?: Task | null; // Optional for create mode
  projectId?: string; // For file search functionality
  onCreateTask?: (title: string, description: string, maxReward: number) => Promise<void>;
  onCreateAndStartTask?: (
    title: string,
    description: string,
    maxReward: number
  ) => Promise<void>;
  onUpdateTask?: (
    title: string,
    description: string,
    status: TaskStatus
  ) => Promise<void>;
}

export function TaskFormDialog({
  isOpen,
  onOpenChange,
  task,
  projectId,
  onCreateTask,
  onCreateAndStartTask,
  onUpdateTask,
}: TaskFormDialogProps) {
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [maxReward, setMaxReward] = useState<number>(0);
  const [status, setStatus] = useState<TaskStatus>('todo');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isSubmittingAndStart, setIsSubmittingAndStart] = useState(false);

  const isEditMode = Boolean(task);

  useEffect(() => {
    if (task) {
      // Edit mode - populate with existing task data
      setTitle(task.title);
      setDescription(task.description || '');
      setStatus(task.status);
    } else {
      // Create mode - reset to defaults
      setTitle('');
      setDescription('');
      setMaxReward(0);
      setStatus('todo');
    }
  }, [task, isOpen]);

  const handleSubmit = async () => {
    if (!title.trim()) return;
    if (!isEditMode && maxReward <= 0) return;

    setIsSubmitting(true);
    try {
      if (isEditMode && onUpdateTask) {
        await onUpdateTask(title, description, status);
      } else if (!isEditMode && onCreateTask) {
        await onCreateTask(title, description, maxReward);
      }

      // Reset form on successful creation
      if (!isEditMode) {
        setTitle('');
        setDescription('');
        setMaxReward(0);
        setStatus('todo');
      }

      onOpenChange(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCreateAndStart = useCallback(async () => {
    if (!title.trim()) return;
    if (!isEditMode && maxReward <= 0) return;

    setIsSubmittingAndStart(true);
    try {
      if (!isEditMode && onCreateAndStartTask) {
        await onCreateAndStartTask(title, description, maxReward);
      }

      // Reset form on successful creation
      setTitle('');
      setDescription('');
      setMaxReward(0);
      setStatus('todo');

      onOpenChange(false);
    } finally {
      setIsSubmittingAndStart(false);
    }
  }, [
    title,
    description,
    maxReward,
    isEditMode,
    onCreateAndStartTask,
    onOpenChange,
  ]);

  const handleCancel = useCallback(() => {
    // Reset form state when canceling
    if (task) {
      setTitle(task.title);
      setDescription(task.description || '');
      setStatus(task.status);
    } else {
      setTitle('');
      setDescription('');
      setMaxReward(0);
      setStatus('todo');
    }
    onOpenChange(false);
  }, [task, onOpenChange]);

  // Handle keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // ESC to close dialog (prevent it from reaching TaskDetailsPanel)
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        handleCancel();
        return;
      }

      // Command/Ctrl + Enter to Create & Start (create mode) or Save (edit mode)
      if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
        if (
          !isEditMode &&
          onCreateAndStartTask &&
          title.trim() &&
          !isSubmitting &&
          !isSubmittingAndStart
        ) {
          event.preventDefault();
          handleCreateAndStart();
        } else if (
          isEditMode &&
          title.trim() &&
          !isSubmitting &&
          !isSubmittingAndStart
        ) {
          event.preventDefault();
          handleSubmit();
        }
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown, true); // Use capture phase to get priority
      return () => document.removeEventListener('keydown', handleKeyDown, true);
    }
  }, [
    isOpen,
    isEditMode,
    onCreateAndStartTask,
    title,
    isSubmitting,
    isSubmittingAndStart,
    handleCreateAndStart,
    handleCancel,
  ]);

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isEditMode ? 'Edit Task' : 'Create New Task'}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <div>
            <Label htmlFor="task-title">Title</Label>
            <Input
              id="task-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Enter task title"
              disabled={isSubmitting || isSubmittingAndStart}
            />
          </div>

          <div>
            <Label htmlFor="task-description">Description</Label>
            <FileSearchTextarea
              value={description}
              onChange={setDescription}
              placeholder="Enter task description (optional). Type @ to search files."
              rows={3}
              disabled={isSubmitting || isSubmittingAndStart}
              projectId={projectId}
            />
          </div>

          {!isEditMode && (
            <div>
              <Label htmlFor="max-reward">Max Reward *</Label>
              <Input
                id="max-reward"
                type="number"
                min="1"
                step="1"
                value={maxReward || ''}
                onChange={(e) => setMaxReward(Number(e.target.value) || 0)}
                placeholder="Enter maximum reward amount"
                disabled={isSubmitting || isSubmittingAndStart}
              />
              <p className="text-sm text-muted-foreground mt-1">
                Maximum reward amount for this task (required)
              </p>
            </div>
          )}

          {isEditMode && (
            <div>
              <Label htmlFor="task-status">Status</Label>
              <Select
                value={status}
                onValueChange={(value) => setStatus(value as TaskStatus)}
                disabled={isSubmitting || isSubmittingAndStart}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="todo">To Do</SelectItem>
                  <SelectItem value="inprogress">In Progress</SelectItem>
                  <SelectItem value="inreview">In Review</SelectItem>
                  <SelectItem value="done">Done</SelectItem>
                  <SelectItem value="cancelled">Cancelled</SelectItem>
                </SelectContent>
              </Select>
            </div>
          )}

          <div className="flex justify-end space-x-2">
            <Button
              variant="outline"
              onClick={handleCancel}
              disabled={isSubmitting || isSubmittingAndStart}
            >
              Cancel
            </Button>
            {isEditMode ? (
              <Button
                onClick={handleSubmit}
                disabled={isSubmitting || !title.trim()}
              >
                {isSubmitting ? 'Updating...' : 'Update Task'}
              </Button>
            ) : (
              <>
                <Button
                  variant="outline"
                  onClick={handleSubmit}
                  disabled={
                    isSubmitting || isSubmittingAndStart || !title.trim() || maxReward <= 0
                  }
                >
                  {isSubmitting ? 'Creating...' : 'Create Task'}
                </Button>
                {onCreateAndStartTask && (
                  <Button
                    onClick={handleCreateAndStart}
                    disabled={
                      isSubmitting || isSubmittingAndStart || !title.trim() || maxReward <= 0
                    }
                  >
                    {isSubmittingAndStart
                      ? 'Creating & Starting...'
                      : 'Create & Start'}
                  </Button>
                )}
              </>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
