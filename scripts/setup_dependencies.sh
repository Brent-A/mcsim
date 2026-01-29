#!/bin/bash
#
# Setup build dependencies for MCSim.
#
# This script downloads and sets up the external dependencies required to build
# and run MCSim:
#
#     1. ITM (Irregular Terrain Model) library from NTIA
#     2. Rerun visualization tool (optional)
#     3. DEM (Digital Elevation Model) data from USGS (optional - AWS tiles are used by default)
#
# The MeshCore submodule must be initialized separately using:
#     git submodule update --init --recursive
#
# Usage:
#     ./setup_dependencies.sh [OPTIONS]
#
# Options:
#     --skip-dem              Skip downloading DEM data files (default behavior - AWS tiles are used instead)
#     --include-dem           Include DEM data files
#     --skip-itm              Skip downloading the ITM library
#     --skip-rerun            Skip downloading the Rerun viewer (optional dependency)
#     --itm-source URL        Alternative GitHub repo URL for ITM library (e.g., https://github.com/usrflo/itm-linux)
#     --dem-csv-path PATH     Path to the CSV file describing DEM tiles to download (default: dem_data/data.csv)
#     --force                 Force re-download of dependencies even if they already exist
#
# Examples:
#     ./setup_dependencies.sh
#         Downloads all dependencies.
#
#     ./setup_dependencies.sh --skip-rerun
#         Downloads required dependencies only, skipping optional Rerun viewer.
#
#     ./setup_dependencies.sh --force
#         Re-downloads all dependencies even if they exist.
#
#     ./setup_dependencies.sh --itm-source https://github.com/usrflo/itm-linux
#         Uses alternative source for ITM library (for Linux/macOS binaries).
#

set -e

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
GRAY='\033[0;37m'
NC='\033[0m' # No Color

# Parse command line arguments
SKIP_DEM=true
SKIP_ITM=false
SKIP_RERUN=false
DEM_CSV_PATH=""
FORCE=false
ITM_SOURCE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-dem)
            SKIP_DEM=true
            shift
            ;;
        --include-dem)
            SKIP_DEM=false
            shift
            ;;
        --skip-itm)
            SKIP_ITM=true
            shift
            ;;
        --skip-rerun)
            SKIP_RERUN=true
            shift
            ;;
        --itm-source)
            ITM_SOURCE="$2"
            shift 2
            ;;
        --dem-csv-path)
            DEM_CSV_PATH="$2"
            shift 2
            ;;
        --force)
            FORCE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Get script and project directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}MCSim Dependency Setup${NC}"
echo -e "${CYAN}========================================${NC}"
echo "Project root: $PROJECT_ROOT"
echo ""

# Helper function to download a file
download_file() {
    local url="$1"
    local output_path="$2"
    local description="$3"
    
    echo -e "  ${YELLOW}Downloading $description...${NC}"
    echo "    URL: $url"
    echo "    Output: $output_path"
    
    local parent_dir
    parent_dir="$(dirname "$output_path")"
    if [[ ! -d "$parent_dir" ]]; then
        mkdir -p "$parent_dir"
    fi
    
    if curl -L -o "$output_path" "$url" 2>/dev/null; then
        echo -e "    ${GREEN}Done!${NC}"
        return 0
    else
        echo -e "    ${RED}Failed to download from $url${NC}"
        return 1
    fi
}

# Helper function to extract archive files (zip or tar.gz)
extract_archive() {
    local archive_path="$1"
    local destination_path="$2"
    
    echo -e "  ${YELLOW}Extracting to $destination_path...${NC}"
    
    if [[ ! -d "$destination_path" ]]; then
        mkdir -p "$destination_path"
    fi
    
    # Determine archive type by extension
    if [[ "$archive_path" == *.tar.gz ]] || [[ "$archive_path" == *.tgz ]]; then
        if tar -xzf "$archive_path" -C "$destination_path"; then
            echo -e "    ${GREEN}Done!${NC}"
            return 0
        else
            echo -e "    ${RED}Failed to extract tar.gz archive${NC}"
            return 1
        fi
    elif [[ "$archive_path" == *.zip ]]; then
        if unzip -q -o "$archive_path" -d "$destination_path"; then
            echo -e "    ${GREEN}Done!${NC}"
            return 0
        else
            echo -e "    ${RED}Failed to extract zip archive${NC}"
            return 1
        fi
    else
        echo -e "    ${RED}Unsupported archive format: $archive_path${NC}"
        return 1
    fi
}

# ============================================================================
# Check MeshCore submodule
# ============================================================================
echo -e "${CYAN}Checking MeshCore submodule...${NC}"
MESHCORE_PATH="$PROJECT_ROOT/MeshCore"
MESHCORE_SRC="$MESHCORE_PATH/src"

if [[ ! -d "$MESHCORE_SRC" ]]; then
    echo -e "  ${YELLOW}WARNING: MeshCore submodule not initialized!${NC}"
    echo -e "  ${YELLOW}Run: git submodule update --init --recursive${NC}"
    echo ""
else
    echo -e "  ${GREEN}MeshCore submodule found.${NC}"
fi
echo ""

# ============================================================================
# Download ITM Library
# ============================================================================
if [[ "$SKIP_ITM" != "true" ]]; then
    echo -e "${CYAN}Setting up ITM (Irregular Terrain Model) library...${NC}"
    
    ITM_DIR="$PROJECT_ROOT/itm"
    ITM_LIB="$ITM_DIR/libitm.so"
    
    if [[ -f "$ITM_LIB" ]] && [[ "$FORCE" != "true" ]]; then
        echo -e "  ${GREEN}ITM library already exists at $ITM_LIB${NC}"
        echo -e "  ${GRAY}Use --force to re-download${NC}"
    else
        # Determine the source repository
        if [[ -n "$ITM_SOURCE" ]]; then
            echo -e "  ${GRAY}ITM library source: $ITM_SOURCE${NC}"
            # Extract owner/repo from GitHub URL, removing trailing slashes
            ITM_REPO=$(echo "$ITM_SOURCE" | sed 's|https://github.com/||' | sed 's|/$||')
        else
            echo -e "  ${GRAY}ITM library source: https://github.com/NTIA/itm${NC}"
            ITM_REPO="NTIA/itm"
        fi
        
        # Create itm directory
        if [[ ! -d "$ITM_DIR" ]]; then
            mkdir -p "$ITM_DIR"
        fi
        
        # Fetch latest release from GitHub
        ITM_RELEASES_URL="https://api.github.com/repos/${ITM_REPO}/releases/latest"
        
        echo -e "  ${YELLOW}Fetching latest ITM release info...${NC}"
        echo -e "  ${GRAY}API URL: ${ITM_RELEASES_URL}${NC}"
        
        release_info=$(curl -s "$ITM_RELEASES_URL" 2>/dev/null)
        
        if [[ -z "$release_info" ]]; then
            echo -e "  ${RED}Failed to fetch ITM release info (empty response)${NC}"
            echo -e "  ${YELLOW}You may need to build ITM manually from https://github.com/${ITM_REPO}${NC}"
            echo -e "  ${YELLOW}Place the compiled libitm.so in: $ITM_DIR${NC}"
            exit 1
        fi
        
        # Check if the API returned an error message
        api_error=$(echo "$release_info" | grep -o '"message":"[^"]*"' | head -1 | sed 's/"message":"//;s/"$//')
        if [[ -n "$api_error" ]]; then
            echo -e "  ${RED}GitHub API Error: ${api_error}${NC}"
            echo -e "  ${YELLOW}Full API response (first 500 chars):${NC}"
            echo -e "  ${GRAY}$(echo "$release_info" | head -c 500)${NC}"
            echo -e "  ${YELLOW}You may need to build ITM manually from https://github.com/${ITM_REPO}${NC}"
            echo -e "  ${YELLOW}Place the compiled libitm.so in: $ITM_DIR${NC}"
            exit 1
        fi
        
        # Extract and display release tag
        # The pattern must account for optional spaces after the colon
        release_tag=$(echo "$release_info" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"//;s/"$//')
        
        # If tag_name is not found, try to extract from html_url as fallback
        if [[ -z "$release_tag" ]]; then
            release_tag=$(echo "$release_info" | grep -o '"html_url"[[:space:]]*:[[:space:]]*"[^"]*releases/tag/[^"]*"' | sed 's|.*releases/tag/||;s|"$||')
        fi
        
        if [[ -z "$release_tag" ]]; then
            echo -e "  ${RED}No release tag found in API response${NC}"
            echo -e "  ${YELLOW}API response (first 1000 chars):${NC}"
            echo -e "  ${GRAY}$(echo "$release_info" | head -c 1000)${NC}"
            echo -e "  ${YELLOW}Searching for tag_name in full response...${NC}"
            if echo "$release_info" | grep -q '"tag_name"'; then
                echo -e "  ${CYAN}Found tag_name field in response (beyond first 1000 chars)${NC}"
                echo -e "  ${GRAY}$(echo "$release_info" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"')${NC}"
            else
                echo -e "  ${RED}tag_name field not found anywhere in response${NC}"
            fi
            echo -e "  ${YELLOW}You may need to build ITM manually from https://github.com/${ITM_REPO}${NC}"
            echo -e "  ${YELLOW}Place the compiled libitm.so in: $ITM_DIR${NC}"
            exit 1
        fi
        
        echo -e "  ${CYAN}Release tag: ${release_tag}${NC}"
        
        # Extract and display all available assets
        echo -e "  ${CYAN}Available assets:${NC}"
        echo "$release_info" | grep -o '"name"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*"name"[[:space:]]*:[[:space:]]*"//;s/"$//' | while read -r asset_name; do
            echo -e "    - ${GRAY}${asset_name}${NC}"
        done
        echo ""
        
        # Try multiple patterns to find Linux binary assets
        # Pattern 1: itm-linux-x86_64.tar.gz (new format)
        linux_asset=$(echo "$release_info" | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*itm-linux-x86_64[^"]*\.tar\.gz"' | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"//;s/"$//' | head -1)
            
            if [[ -z "$linux_asset" ]]; then
                # Pattern 2: linux-x86_64.zip or linux-x86_64.tar.gz
                linux_asset=$(echo "$release_info" | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*linux-x86_64[^"]*\.\(zip\|tar\.gz\)"' | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"//;s/"$//' | head -1)
            fi
            
            if [[ -z "$linux_asset" ]]; then
                # Pattern 3: linux_x86_64.zip or tar.gz
                linux_asset=$(echo "$release_info" | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*linux_x86_64[^"]*\.\(zip\|tar\.gz\)"' | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"//;s/"$//' | head -1)
            fi
            
            if [[ -z "$linux_asset" ]]; then
                # Pattern 4: linux.zip, Linux.zip, linux.tar.gz, or Linux.tar.gz
                linux_asset=$(echo "$release_info" | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*[Ll]inux[^"]*\.\(zip\|tar\.gz\)"' | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"//;s/"$//' | head -1)
            fi
            
            if [[ -z "$linux_asset" ]]; then
                # Pattern 5: Any file with x86_64 (zip or tar.gz)
                linux_asset=$(echo "$release_info" | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*x86_64[^"]*\.\(zip\|tar\.gz\)"' | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"//;s/"$//' | head -1)
            fi
            
            if [[ -z "$linux_asset" ]]; then
                # Pattern 6: Any .so file directly
                linux_asset=$(echo "$release_info" | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*\.so"' | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"//;s/"$//' | head -1)
            fi
            
            if [[ -n "$linux_asset" ]]; then
                echo -e "  ${CYAN}Found asset: $linux_asset${NC}"
                
                # Check if it's a .so file or an archive file
                if [[ "$linux_asset" == *.so ]]; then
                    # Download .so file directly
                    if download_file "$linux_asset" "$ITM_LIB" "ITM library"; then
                        echo -e "  ${GREEN}ITM library installed successfully${NC}"
                    else
                        echo -e "  ${RED}Failed to download ITM library${NC}"
                        exit 1
                    fi
                else
                    # Download and extract archive (zip or tar.gz)
                    # Preserve the file extension for proper extraction
                    if [[ "$linux_asset" == *.tar.gz ]]; then
                        archive_path="$ITM_DIR/itm_temp.tar.gz"
                    elif [[ "$linux_asset" == *.zip ]]; then
                        archive_path="$ITM_DIR/itm_temp.zip"
                    else
                        archive_path="$ITM_DIR/itm_temp_archive"
                    fi
                    
                    if download_file "$linux_asset" "$archive_path" "ITM release"; then
                        if extract_archive "$archive_path" "$ITM_DIR"; then
                            rm -f "$archive_path"
                            
                            # Verify the library file exists after extraction
                            if [[ -f "$ITM_LIB" ]]; then
                                echo -e "  ${GREEN}ITM library installed successfully${NC}"
                            else
                                echo -e "  ${RED}ITM library not found after extraction${NC}"
                                echo -e "  ${YELLOW}Files extracted:${NC}"
                                ls -la "$ITM_DIR"
                                exit 1
                            fi
                        else
                            echo -e "  ${RED}Failed to extract ITM archive${NC}"
                            exit 1
                        fi
                    else
                        echo -e "  ${RED}Failed to download ITM release${NC}"
                        exit 1
                    fi
                fi
            else
                echo -e "  ${RED}No pre-built Linux binary found in latest release.${NC}"
                echo -e "  ${YELLOW}You may need to build ITM manually from https://github.com/${ITM_REPO}${NC}"
                echo -e "  ${YELLOW}Place the compiled libitm.so in: $ITM_DIR${NC}"
                exit 1
            fi
    fi
    echo ""
fi

# ============================================================================
# Download Rerun Viewer (Optional)
# ============================================================================
if [[ "$SKIP_RERUN" != "true" ]]; then
    echo -e "${CYAN}Setting up Rerun viewer (optional)...${NC}"
    
    RERUN_DIR="$PROJECT_ROOT/rerun"
    RERUN_BIN="$RERUN_DIR/rerun"
    
    if [[ -f "$RERUN_BIN" ]] && [[ "$FORCE" != "true" ]]; then
        echo -e "  ${GREEN}Rerun viewer already exists at $RERUN_BIN${NC}"
        echo -e "  ${GRAY}Use --force to re-download${NC}"
    else
        echo -e "  ${GRAY}Rerun source: https://github.com/rerun-io/rerun${NC}"
        
        # Create rerun directory
        if [[ ! -d "$RERUN_DIR" ]]; then
            mkdir -p "$RERUN_DIR"
        fi
        
        # Fetch latest release from GitHub
        RERUN_RELEASES_URL="https://api.github.com/repos/rerun-io/rerun/releases/latest"
        
        echo -e "  ${YELLOW}Fetching latest Rerun release info...${NC}"
        
        release_info=$(curl -s "$RERUN_RELEASES_URL" 2>/dev/null)
        
        if [[ -z "$release_info" ]]; then
            echo -e "  ${RED}Failed to fetch Rerun release info${NC}"
            echo -e "  ${YELLOW}You can install Rerun via cargo: cargo install rerun-cli${NC}"
        else
            version=$(echo "$release_info" | grep -o '"tag_name":"[^"]*"' | head -1 | sed 's/"tag_name":"//;s/"$//')
            echo -e "  ${GRAY}Latest version: $version${NC}"
            
            # Find Linux x86_64 asset
            linux_asset=$(echo "$release_info" | grep -o '"browser_download_url":"[^"]*linux[^"]*x86_64[^"]*\.tar\.gz"' | head -1 | sed 's/"browser_download_url":"//;s/"$//')
            
            if [[ -z "$linux_asset" ]]; then
                # Try alternative pattern
                linux_asset=$(echo "$release_info" | grep -o '"browser_download_url":"[^"]*linux-x64[^"]*\.tar\.gz"' | head -1 | sed 's/"browser_download_url":"//;s/"$//')
            fi
            
            if [[ -n "$linux_asset" ]]; then
                archive_path="$RERUN_DIR/rerun_temp.tar.gz"
                if download_file "$linux_asset" "$archive_path" "Rerun viewer"; then
                    if tar -xzf "$archive_path" -C "$RERUN_DIR"; then
                        echo -e "    ${GREEN}Extracted!${NC}"
                        rm -f "$archive_path"
                        
                        # Find and move the rerun binary to the expected location
                        found_bin=$(find "$RERUN_DIR" -name "rerun" -type f 2>/dev/null | head -1)
                        if [[ -n "$found_bin" ]] && [[ "$found_bin" != "$RERUN_BIN" ]]; then
                            mv "$found_bin" "$RERUN_BIN"
                            chmod +x "$RERUN_BIN"
                            # Clean up extracted subdirectories
                            find "$RERUN_DIR" -maxdepth 1 -type d -not -name "rerun" -exec rm -rf {} \; 2>/dev/null || true
                        fi
                    fi
                fi
            else
                echo -e "  ${YELLOW}No Linux x86_64 binary found in latest release.${NC}"
                echo -e "  ${YELLOW}You can install Rerun via cargo: cargo install rerun-cli${NC}"
            fi
        fi
    fi
    echo ""
fi

# ============================================================================
# Download DEM Data (Optional - AWS tiles are used by default)
# ============================================================================
if [[ "$SKIP_DEM" != "true" ]]; then
    echo -e "${CYAN}Setting up DEM (Digital Elevation Model) data...${NC}"
    echo -e "  ${GRAY}NOTE: DEM data is optional. AWS terrain tiles are used by default.${NC}"
    echo -e "  ${GRAY}Use --elevation-source local_dem to use local DEM files.${NC}"
    
    DEM_DIR="$PROJECT_ROOT/dem_data"
    
    # Determine CSV path
    if [[ -z "$DEM_CSV_PATH" ]]; then
        DEM_CSV_PATH="$DEM_DIR/data.csv"
    fi
    
    if [[ ! -f "$DEM_CSV_PATH" ]]; then
        echo -e "  ${YELLOW}WARNING: DEM data CSV not found at $DEM_CSV_PATH${NC}"
        echo -e "  ${YELLOW}Export a CSV from https://apps.nationalmap.gov/downloader/${NC}"
        echo -e "  ${YELLOW}and save it to $DEM_CSV_PATH${NC}"
    else
        # Check if we have .tif files already
        tif_count=$(find "$DEM_DIR" -maxdepth 1 -name "*.tif" 2>/dev/null | wc -l)
        
        if [[ $tif_count -gt 0 ]] && [[ "$FORCE" != "true" ]]; then
            echo -e "  ${GREEN}Found $tif_count existing DEM .tif files in $DEM_DIR${NC}"
            echo -e "  ${GRAY}Use --force to re-download${NC}"
        else
            echo -e "  ${YELLOW}Running DEM download script...${NC}"
            
            fetch_script="$SCRIPT_DIR/fetch_usgs_dem.py"
            
            if [[ -f "$fetch_script" ]]; then
                # Check for Python
                python_cmd=""
                if command -v python &> /dev/null; then
                    python_cmd="python"
                elif command -v python3 &> /dev/null; then
                    python_cmd="python3"
                fi
                
                if [[ -n "$python_cmd" ]]; then
                    skip_existing=""
                    if [[ "$FORCE" != "true" ]]; then
                        skip_existing="--skip-existing"
                    fi
                    pushd "$PROJECT_ROOT" > /dev/null
                    $python_cmd "$fetch_script" "$DEM_CSV_PATH" --output-dir "$DEM_DIR" $skip_existing --use-urllib || true
                    popd > /dev/null
                else
                    echo -e "  ${YELLOW}Python not found. Install Python to download DEM data.${NC}"
                    echo -e "  ${YELLOW}Or manually download the .tif files from the URLs in $DEM_CSV_PATH${NC}"
                fi
            else
                echo -e "  ${YELLOW}DEM fetch script not found at $fetch_script${NC}"
            fi
        fi
    fi
    echo ""
fi

# ============================================================================
# Summary
# ============================================================================
echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}Setup Complete!${NC}"
echo -e "${CYAN}========================================${NC}"
echo ""
echo -e "${WHITE}Dependency Status:${NC}"

# Check MeshCore
if [[ -d "$PROJECT_ROOT/MeshCore/src" ]]; then
    echo -e "  MeshCore submodule: ${GREEN}OK${NC}"
else
    echo -e "  MeshCore submodule: ${RED}MISSING${NC}"
fi

# Check ITM
if [[ -f "$PROJECT_ROOT/itm/libitm.so" ]]; then
    echo -e "  ITM library:        ${GREEN}OK${NC}"
else
    echo -e "  ITM library:        ${RED}MISSING${NC}"
fi

# Check Rerun
if [[ -f "$PROJECT_ROOT/rerun/rerun" ]]; then
    echo -e "  Rerun viewer:       ${GREEN}OK${NC}"
else
    echo -e "  Rerun viewer:       ${YELLOW}MISSING (optional)${NC}"
fi

# Check DEM
dem_tif_count=$(find "$PROJECT_ROOT/dem_data" -maxdepth 1 -name "*.tif" 2>/dev/null | wc -l)
if [[ $dem_tif_count -gt 0 ]]; then
    echo -e "  DEM data:           ${GREEN}OK ($dem_tif_count files)${NC}"
else
    echo -e "  DEM data:           ${GRAY}Not downloaded (optional - AWS tiles used by default)${NC}"
fi

echo ""
echo -e "${WHITE}Next steps:${NC}"
echo "  1. Build the project: cargo build --release --features rerun"
echo "  2. Run tests: cargo test"
echo "  3. Create release: ./scripts/create_release.sh"
echo ""
