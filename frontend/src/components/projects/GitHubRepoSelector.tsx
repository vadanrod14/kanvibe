import { useEffect, useState } from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Loader } from '@/components/ui/loader';
import { GitHubRepository, githubAuthApi } from '@/lib/api';
import { Search, Lock, Globe } from 'lucide-react';
import { useConfig } from '@/components/config-provider';

interface GitHubRepoSelectorProps {
  open: boolean;
  onClose: () => void;
  onSelect: (repo: GitHubRepository) => void;
}

export function GitHubRepoSelector({
  open,
  onClose,
  onSelect,
}: GitHubRepoSelectorProps) {
  const { config } = useConfig();
  const [repositories, setRepositories] = useState<GitHubRepository[]>([]);
  const [filteredRepos, setFilteredRepos] = useState<GitHubRepository[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState('');

  const isAuthenticated = !!(config?.github?.username && config?.github?.token);

  useEffect(() => {
    if (open && isAuthenticated) {
      fetchRepositories();
    }
  }, [open, isAuthenticated]);

  useEffect(() => {
    if (searchTerm) {
      const filtered = repositories.filter(repo =>
        repo.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
        repo.full_name.toLowerCase().includes(searchTerm.toLowerCase()) ||
        (repo.description?.toLowerCase().includes(searchTerm.toLowerCase()))
      );
      setFilteredRepos(filtered);
    } else {
      setFilteredRepos(repositories);
    }
  }, [searchTerm, repositories]);

  const fetchRepositories = async () => {
    setLoading(true);
    setError(null);
    try {
      const repos = await githubAuthApi.getRepositories();
      setRepositories(repos);
      setFilteredRepos(repos);
    } catch (e: any) {
      console.error('Failed to fetch repositories:', e);
      setError(e?.message || 'Failed to fetch repositories');
    } finally {
      setLoading(false);
    }
  };

  const handleSelect = (repo: GitHubRepository) => {
    onSelect(repo);
    onClose();
  };

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString();
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-[600px] h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Select GitHub Repository</DialogTitle>
          <DialogDescription>
            Choose a repository from your GitHub account to clone
          </DialogDescription>
        </DialogHeader>

        {!isAuthenticated ? (
          <Alert>
            <AlertDescription>
              Please authenticate with GitHub first to view your repositories.
            </AlertDescription>
          </Alert>
        ) : (
          <div className="flex-1 flex flex-col space-y-4 min-h-0">
            <div className="space-y-2">
              <Label htmlFor="search">Search repositories</Label>
              <div className="relative">
                <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 text-muted-foreground h-4 w-4" />
                <Input
                  id="search"
                  type="text"
                  placeholder="Search by name or description..."
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  className="pl-10"
                />
              </div>
            </div>

            {loading ? (
              <div className="flex-1 flex items-center justify-center">
                <Loader message="Loading repositories..." size={32} />
              </div>
            ) : error ? (
              <Alert variant="destructive">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : (
              <div className="flex-1 overflow-y-auto space-y-2 min-h-0">
                {filteredRepos.length === 0 ? (
                  <div className="text-center text-muted-foreground py-8">
                    {searchTerm
                      ? `No repositories found matching "${searchTerm}"`
                      : 'No repositories found'}
                  </div>
                ) : (
                  filteredRepos.map((repo) => (
                    <div
                      key={repo.id}
                      className="border rounded-lg p-4 hover:bg-muted/50 cursor-pointer transition-colors"
                      onClick={() => handleSelect(repo)}
                    >
                      <div className="flex items-start justify-between">
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center space-x-2 mb-1">
                            <h3 className="font-medium text-sm truncate">
                              {repo.full_name}
                            </h3>
                            <div className="flex items-center space-x-1">
                              {repo.private ? (
                                <Lock className="h-3 w-3 text-muted-foreground" />
                              ) : (
                                <Globe className="h-3 w-3 text-muted-foreground" />
                              )}
                            </div>
                          </div>
                          {repo.description && (
                            <p className="text-xs text-muted-foreground mb-2 line-clamp-2">
                              {repo.description}
                            </p>
                          )}
                          <div className="flex items-center space-x-4 text-xs text-muted-foreground">
                            {repo.language && (
                              <span className="flex items-center space-x-1">
                                <div
                                  className="w-2 h-2 rounded-full"
                                  style={{
                                    backgroundColor: getLanguageColor(repo.language),
                                  }}
                                />
                                <span>{repo.language}</span>
                              </span>
                            )}
                            <span>Updated {formatDate(repo.updated_at)}</span>
                          </div>
                        </div>
                      </div>
                    </div>
                  ))
                )}
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

// Simple language color mapping (you could expand this)
function getLanguageColor(language: string): string {
  const colors: Record<string, string> = {
    TypeScript: '#3178c6',
    JavaScript: '#f1e05a',
    Python: '#3572A5',
    Java: '#b07219',
    'C++': '#f34b7d',
    C: '#555555',
    Go: '#00ADD8',
    Rust: '#dea584',
    PHP: '#4F5D95',
    Ruby: '#701516',
    Swift: '#fa7343',
    Kotlin: '#A97BFF',
    HTML: '#e34c26',
    CSS: '#563d7c',
    Shell: '#89e051',
  };
  return colors[language] || '#858585';
}