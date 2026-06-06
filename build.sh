#!/usr/bin/env bash
# OpenCrabs Build Script
#
# Usage:
#   ./build.sh                    # Build with default features (full)
#   ./build.sh <profile>          # Build with a named profile
#   ./build.sh --list             # List available profiles
#   ./build.sh --release          # Build release with default features
#   ./build.sh --release <profile># Build release with named profile
#
# Examples:
#   ./build.sh minimal            # Core tools only, no channels
#   ./build.sh chatbot            # No tools - pure chatbot
#   ./build.sh telegram-agent     # Core tools + Telegram
#   ./build.sh --release full     # Full release build

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILES_FILE="$SCRIPT_DIR/build-profiles.toml"
CARGO="${CARGO:-cargo}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

usage() {
    echo -e "${BLUE}OpenCrabs Build Script${NC}"
    echo ""
    echo "Usage:"
    echo "  $0                    Build with default features (full)"
    echo "  $0 <profile>          Build with a named profile"
    echo "  $0 --list             List available profiles"
    echo "  $0 --release          Build release with default features"
    echo "  $0 --release <profile># Build release with named profile"
    echo ""
    echo "Profiles are defined in build-profiles.toml"
}

list_profiles() {
    if [[ ! -f "$PROFILES_FILE" ]]; then
        echo -e "${RED}Error: $PROFILES_FILE not found${NC}"
        exit 1
    fi

    echo -e "${BLUE}Available build profiles:${NC}"
    echo ""
    
    # Parse TOML to extract profile names and descriptions
    # Look for lines like [profilename] followed by description = "..."
    awk '
    /^\[[a-z]/ {
        gsub(/[\[\]]/, "")
        profile = $0
        next
    }
    /^description[[:space:]]*=/ && profile != "" {
        gsub(/^description[[:space:]]*=[[:space:]]*"/, "")
        gsub(/"$/, "")
        printf "  %-20s %s\n", profile, $0
        profile = ""
    }
    ' "$PROFILES_FILE" | while read -r line; do
        # Colorize the profile name
        profile_name=$(echo "$line" | awk '{print $1}')
        desc=$(echo "$line" | cut -d' ' -f2-)
        printf "  ${GREEN}%-20s${NC} %s\n" "$profile_name" "$desc"
    done
    
    echo ""
    echo "Edit $PROFILES_FILE to add custom profiles."
}

# Parse a profile from build-profiles.toml
parse_profile() {
    local profile="$1"
    
    if [[ ! -f "$PROFILES_FILE" ]]; then
        echo -e "${RED}Error: $PROFILES_FILE not found${NC}"
        exit 1
    fi
    
    # Check if profile exists
    if ! grep -q "^\[$profile\]" "$PROFILES_FILE"; then
        echo -e "${RED}Error: Profile '$profile' not found in $PROFILES_FILE${NC}"
        echo ""
        echo "Available profiles:"
        list_profiles
        exit 1
    fi
    
    # Extract profile settings
    local in_section=0
    local default_features="true"
    local features=""
    
    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// }" ]] && continue
        
        # Check for section start
        if [[ "$line" =~ ^\[([^]]+)\] ]]; then
            local section="${BASH_REMATCH[1]}"
            if [[ "$section" == "$profile" ]]; then
                in_section=1
            else
                [[ $in_section -eq 1 ]] && break
            fi
            continue
        fi
        
        # Parse settings within the profile section
        if [[ $in_section -eq 1 ]]; then
            if [[ "$line" =~ ^default_features[[:space:]]*=[[:space:]]*(true|false) ]]; then
                default_features="${BASH_REMATCH[1]}"
            elif [[ "$line" =~ ^features[[:space:]]*= ]]; then
                # Extract features array - handle multi-line
                features="$line"
                while [[ ! "$features" =~ \] ]]; do
                    IFS= read -r next_line
                    features="$features $next_line"
                done
                # Clean up features string
                features=$(echo "$features" | sed 's/features = \[//;s/\]//;s/"//g;s/,/ /g;s/[[:space:]]\+/ /g')
            fi
        fi
    done < "$PROFILES_FILE"
    
    echo "$default_features|$features"
}

build() {
    local profile="${1:-full}"
    local release="${2:-}"
    
    echo -e "${BLUE}Building with profile: ${GREEN}$profile${NC}"
    
    local parsed
    parsed=$(parse_profile "$profile")
    
    local default_features="${parsed%%|*}"
    local features="${parsed#*|}"
    
    # Build cargo command
    local cmd=("$CARGO" build)
    
    if [[ -n "$release" ]]; then
        cmd+=(--release)
    fi
    
    cmd+=(--locked)
    
    if [[ "$default_features" == "false" ]]; then
        cmd+=(--no-default-features)
    fi
    
    if [[ -n "${features// }" ]]; then
        # Convert space-separated features to comma-separated
        local features_csv="${features// /,}"
        cmd+=(--features "$features_csv")
    fi
    
    echo -e "${YELLOW}Running: ${cmd[*]}${NC}"
    echo ""
    
    "${cmd[@]}"
    
    echo ""
    echo -e "${GREEN}✓ Build complete!${NC}"
    
    if [[ -n "$release" ]]; then
        echo -e "Binary: ${BLUE}./target/release/opencrabs${NC}"
    else
        echo -e "Binary: ${BLUE}./target/debug/opencrabs${NC}"
    fi
}

# Main
case "${1:-}" in
    -h|--help)
        usage
        ;;
    -l|--list)
        list_profiles
        ;;
    --release)
        shift
        build "${1:-full}" "--release"
        ;;
    "")
        build "full"
        ;;
    -*)
        echo -e "${RED}Error: Unknown option $1${NC}"
        usage
        exit 1
        ;;
    *)
        build "$1"
        ;;
esac
