#!/bin/bash

# Navigate to your project root
cd clara-cerebellum || { echo "Directory clara-cerebellum not found."; exit 1; }

# Initialize Git repository
git init

# Add all files
git add .

# Create initial commit
git commit -m "Initial commit: scaffolded clara-cerebellum workspace layout"

# Set up main branch
git branch -M main

# Optional: add remote (replace with your actual repo URL)
# git remote add origin git@github.com:your-username/clara-cerebellum.git

# Optional: push to remote
# git push -u origin main

echo "✅ Git repository initialized and first commit created."

