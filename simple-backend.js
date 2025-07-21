const express = require('express');
const cors = require('cors');
const app = express();

// CORS configuration
const corsOptions = {
  origin: [
    'https://vibe-kanban-prod.web.app',
    'https://grouplang-firebase.web.app',
    'https://grouplang-450317.web.app',
    'https://grouplang-450317.firebaseapp.com',
    'http://localhost:3000',
    'http://127.0.0.1:3000'
  ],
  credentials: true,
  methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
  allowedHeaders: ['Content-Type', 'Authorization']
};

app.use(cors(corsOptions));
app.use(express.json());

// Health check
app.get('/api/health', (req, res) => {
  res.json({ status: 'ok', message: 'Simple backend running', timestamp: new Date().toISOString() });
});

// Config endpoint
app.get('/api/config', (req, res) => {
  res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Expires', '0');
  res.json({
    success: true,
    data: {
      github_app_client_id: process.env.GITHUB_APP_CLIENT_ID || 'Ov23liThHJo9xbl97ZUF',
      sentry_dsn: process.env.SENTRY_DSN || '',
      analytics_enabled: false,
      features: {},
      github: userConfig.github
    },
    timestamp: new Date().toISOString()
  });
});

// Projects endpoint
app.get('/api/projects', (req, res) => {
  res.json({
    success: true,
    data: [],
    message: 'No projects yet - backend is starting up'
  });
});

// In-memory storage for user sessions (for demo purposes)
let userConfig = {
  github: {
    token: null,
    username: null,
    name: null,
    email: null,
    avatar_url: null
  }
};

// Auth check endpoint
app.get('/api/auth/github/check', (req, res) => {
  const hasToken = !!userConfig.github.token;
  res.json({
    success: true,
    data: { authenticated: hasToken },
    message: hasToken ? 'User is authenticated' : 'User not authenticated'
  });
});

// Save config endpoint
app.post('/api/config', (req, res) => {
  try {
    const updates = req.body;
    
    // Update the stored config
    if (updates.github) {
      userConfig.github = { ...userConfig.github, ...updates.github };
    }
    
    res.json({
      success: true,
      data: {
        github_app_client_id: process.env.GITHUB_APP_CLIENT_ID || 'Ov23liThHJo9xbl97ZUF',
        sentry_dsn: process.env.SENTRY_DSN || '',
        analytics_enabled: false,
        features: {},
        github: userConfig.github
      },
      timestamp: new Date().toISOString()
    });
  } catch (error) {
    console.error('Error saving config:', error);
    res.status(500).json({
      success: false,
      message: 'Failed to save config'
    });
  }
});

// Get current config including GitHub auth status
app.get('/api/config/current', (req, res) => {
  res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Expires', '0');
  
  res.json({
    success: true,
    data: {
      github_app_client_id: process.env.GITHUB_APP_CLIENT_ID || 'Ov23liThHJo9xbl97ZUF',
      sentry_dsn: process.env.SENTRY_DSN || '',
      analytics_enabled: false,
      features: {},
      github: userConfig.github
    },
    timestamp: new Date().toISOString()
  });
});

// GitHub device flow start
app.post('/api/auth/github/device/start', async (req, res) => {
  try {
    const clientId = process.env.GITHUB_APP_CLIENT_ID || 'Ov23liThHJo9xbl97ZUF';
    
    // Call GitHub device flow API
    const response = await fetch('https://github.com/login/device/code', {
      method: 'POST',
      headers: {
        'Accept': 'application/json',
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      body: `client_id=${clientId}&scope=read:user,user:email,repo`
    });
    
    const data = await response.json();
    
    if (!response.ok) {
      throw new Error(`GitHub API error: ${data.error_description || data.error}`);
    }
    
    res.json({
      success: true,
      data: {
        device_code: data.device_code,
        user_code: data.user_code,
        verification_uri: data.verification_uri,
        expires_in: data.expires_in,
        interval: data.interval
      }
    });
  } catch (error) {
    console.error('GitHub device flow start error:', error);
    res.status(500).json({
      success: false,
      message: error.message || 'Failed to start GitHub device flow'
    });
  }
});

// GitHub device flow poll
app.post('/api/auth/github/device/poll', async (req, res) => {
  try {
    const { device_code } = req.body;
    const clientId = process.env.GITHUB_APP_CLIENT_ID || 'Ov23liThHJo9xbl97ZUF';
    const clientSecret = process.env.GITHUB_APP_CLIENT_SECRET || 'b33cad54d9958ceb53b76eeff6cb6f0f649e2195';
    
    if (!device_code) {
      return res.status(400).json({
        success: false,
        message: 'device_code is required'
      });
    }
    
    // Poll GitHub for access token
    const response = await fetch('https://github.com/login/oauth/access_token', {
      method: 'POST',
      headers: {
        'Accept': 'application/json',
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      body: `client_id=${clientId}&device_code=${device_code}&grant_type=urn:ietf:params:oauth:grant-type:device_code&client_secret=${clientSecret}`
    });
    
    const data = await response.json();
    
    if (data.error) {
      if (data.error === 'authorization_pending') {
        return res.json({
          success: false,
          error: 'authorization_pending',
          message: 'authorization_pending'
        });
      } else if (data.error === 'slow_down') {
        return res.json({
          success: false,
          error: 'slow_down',
          message: 'slow_down'
        });
      } else if (data.error === 'expired_token') {
        return res.status(400).json({
          success: false,
          error: 'expired_token',
          message: 'expired_token'
        });
      } else {
        throw new Error(data.error_description || data.error);
      }
    }
    
    if (data.access_token) {
      // Get user info from GitHub
      const userResponse = await fetch('https://api.github.com/user', {
        headers: {
          'Authorization': `Bearer ${data.access_token}`,
          'Accept': 'application/json',
        }
      });
      
      const userInfo = await userResponse.json();
      
      // Save the authentication data in memory
      userConfig.github = {
        token: data.access_token,
        username: userInfo.login,
        name: userInfo.name,
        email: userInfo.email,
        avatar_url: userInfo.avatar_url
      };
      
      res.json({
        success: true,
        data: {
          access_token: data.access_token,
          token_type: data.token_type,
          scope: data.scope,
          user: {
            id: userInfo.id,
            login: userInfo.login,
            name: userInfo.name,
            email: userInfo.email,
            avatar_url: userInfo.avatar_url
          }
        }
      });
    } else {
      throw new Error('No access token received');
    }
  } catch (error) {
    console.error('GitHub device flow poll error:', error);
    res.status(500).json({
      success: false,
      message: error.message || 'Failed to poll GitHub device flow'
    });
  }
});

// MCP Servers endpoint (placeholder)
app.get('/api/mcp-servers', (req, res) => {
  res.json({
    success: true,
    data: [],
    message: 'MCP servers not available in simple backend'
  });
});

// Filesystem endpoints (placeholder)
app.get('/api/filesystem/list', (req, res) => {
  res.json({
    success: true,
    data: {
      files: [],
      directories: []
    },
    message: 'Filesystem access not available in simple backend'
  });
});

app.get('/api/filesystem/*', (req, res) => {
  res.json({
    success: false,
    message: 'Filesystem operations not available in simple backend'
  });
});

// Tasks endpoints (placeholder)
app.get('/api/tasks', (req, res) => {
  res.json({
    success: true,
    data: [],
    message: 'Tasks not available in simple backend'
  });
});

app.post('/api/tasks', (req, res) => {
  res.json({
    success: false,
    message: 'Task creation not available in simple backend'
  });
});

// Attempts endpoints (placeholder)
app.get('/api/tasks/:taskId/attempts', (req, res) => {
  res.json({
    success: true,
    data: [],
    message: 'Task attempts not available in simple backend'
  });
});

// Executors endpoints (placeholder)
app.get('/api/executors', (req, res) => {
  res.json({
    success: true,
    data: [],
    message: 'Executors not available in simple backend'
  });
});

// GitHub repositories endpoint
app.get('/api/github/repos', async (req, res) => {
  try {
    if (!userConfig.github.token) {
      return res.status(401).json({
        success: false,
        message: 'GitHub authentication required'
      });
    }

    // Fetch user's repositories from GitHub
    const response = await fetch('https://api.github.com/user/repos?sort=updated&per_page=100', {
      headers: {
        'Authorization': `Bearer ${userConfig.github.token}`,
        'Accept': 'application/vnd.github.v3+json',
        'User-Agent': 'vibe-kanban-app'
      }
    });

    if (!response.ok) {
      throw new Error(`GitHub API error: ${response.status}`);
    }

    const repos = await response.json();
    
    // Transform to the format expected by frontend
    const formattedRepos = repos.map(repo => ({
      id: repo.id,
      name: repo.name,
      full_name: repo.full_name,
      private: repo.private,
      html_url: repo.html_url,
      clone_url: repo.clone_url,
      ssh_url: repo.ssh_url,
      description: repo.description,
      language: repo.language,
      updated_at: repo.updated_at,
      owner: {
        login: repo.owner.login,
        avatar_url: repo.owner.avatar_url
      }
    }));

    res.json({
      success: true,
      data: formattedRepos
    });
  } catch (error) {
    console.error('Error fetching GitHub repos:', error);
    res.status(500).json({
      success: false,
      message: 'Failed to fetch GitHub repositories'
    });
  }
});

// Catch-all for other API endpoints
app.use('/api/*', (req, res) => {
  res.status(404).json({
    success: false,
    message: `API endpoint ${req.path} not available in simple backend`
  });
});

const port = process.env.PORT || 8080;
app.listen(port, '0.0.0.0', () => {
  console.log(`Simple backend running on port ${port}`);
});