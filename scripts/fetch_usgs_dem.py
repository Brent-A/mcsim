#!/usr/bin/env python3
r"""
Fetch DEM (Digital Elevation Model) data from USGS National Map.

This script reads a CSV file exported from https://apps.nationalmap.gov/downloader/
and downloads the GeoTIFF DEM tiles to a local directory.

Usage:
    python fetch_usgs_dem.py <csv_file> [--output-dir <dir>] [--dry-run]
    
Example:
    python fetch_usgs_dem.py "C:\Users\balle\Downloads\data.csv" --output-dir ./dem_data
"""

import argparse
import csv
import os
import sys
from pathlib import Path
from urllib.parse import urlparse
import hashlib
import time

# Try to import optional dependencies
try:
    import requests
    HAS_REQUESTS = True
except ImportError:
    HAS_REQUESTS = False

try:
    from tqdm import tqdm
    HAS_TQDM = True
except ImportError:
    HAS_TQDM = False


def parse_csv(csv_path: str) -> list[dict]:
    """
    Parse the USGS National Map downloader CSV file.
    
    The CSV from the USGS downloader has no header row.
    Key columns:
        - Column 2: Title (e.g., "USGS 1/3 Arc Second n48w123 20240327")
        - Column 22: File size in bytes
        - Column 25: Download URL (.tif file)
    """
    entries = []
    
    with open(csv_path, 'r', encoding='utf-8') as f:
        reader = csv.reader(f)
        for row_idx, row in enumerate(reader):
            # Find the .tif URL in the row
            tif_url = None
            for cell in row:
                if '.tif' in cell and cell.startswith('http'):
                    tif_url = cell
                    break
            
            if not tif_url:
                print(f"Warning: No .tif URL found in row {row_idx}, skipping")
                continue
            
            # Extract relevant fields
            title = row[2] if len(row) > 2 else f"unknown_{row_idx}"
            
            # Try to get file size (column 22)
            file_size = None
            if len(row) > 22:
                try:
                    file_size = int(row[22])
                except ValueError:
                    pass
            
            # Extract filename from URL
            parsed_url = urlparse(tif_url)
            filename = os.path.basename(parsed_url.path)
            
            entries.append({
                'title': title,
                'url': tif_url,
                'filename': filename,
                'expected_size': file_size,
                'row_index': row_idx
            })
    
    return entries


def format_size(size_bytes: int) -> str:
    """Format file size in human-readable format."""
    for unit in ['B', 'KB', 'MB', 'GB']:
        if abs(size_bytes) < 1024.0:
            return f"{size_bytes:.1f} {unit}"
        size_bytes /= 1024.0
    return f"{size_bytes:.1f} TB"


def download_file(url: str, output_path: Path, expected_size: int | None = None) -> bool:
    """
    Download a file from URL with progress indication.
    
    Returns True if download succeeded, False otherwise.
    """
    if not HAS_REQUESTS:
        print("Error: 'requests' library not installed. Run: pip install requests")
        return False
    
    try:
        # Make request with streaming enabled
        response = requests.get(url, stream=True, timeout=30)
        response.raise_for_status()
        
        # Get total size from headers or use expected size
        total_size = int(response.headers.get('content-length', 0))
        if total_size == 0 and expected_size:
            total_size = expected_size
        
        # Create output directory if needed
        output_path.parent.mkdir(parents=True, exist_ok=True)
        
        # Download with progress bar
        downloaded = 0
        block_size = 8192
        
        if HAS_TQDM and total_size > 0:
            progress = tqdm(
                total=total_size,
                unit='B',
                unit_scale=True,
                desc=output_path.name[:30],
                ncols=80
            )
        else:
            progress = None
            if total_size > 0:
                print(f"  Downloading {format_size(total_size)}...")
            else:
                print(f"  Downloading (size unknown)...")
        
        with open(output_path, 'wb') as f:
            for chunk in response.iter_content(chunk_size=block_size):
                if chunk:
                    f.write(chunk)
                    downloaded += len(chunk)
                    if progress:
                        progress.update(len(chunk))
                    elif not HAS_TQDM:
                        # Simple progress indicator
                        if total_size > 0:
                            pct = (downloaded / total_size) * 100
                            print(f"\r  Progress: {pct:.1f}% ({format_size(downloaded)})", end='', flush=True)
        
        if progress:
            progress.close()
        else:
            print()  # Newline after progress
        
        return True
        
    except requests.exceptions.RequestException as e:
        print(f"  Error downloading: {e}")
        return False


def download_file_urllib(url: str, output_path: Path, expected_size: int | None = None) -> bool:
    """
    Fallback download using urllib (no external dependencies).
    """
    import urllib.request
    import urllib.error
    
    try:
        # Create output directory if needed
        output_path.parent.mkdir(parents=True, exist_ok=True)
        
        print(f"  Connecting to {urlparse(url).netloc}...")
        
        request = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        
        with urllib.request.urlopen(request, timeout=30) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            if total_size == 0 and expected_size:
                total_size = expected_size
            
            if total_size > 0:
                print(f"  File size: {format_size(total_size)}")
            
            downloaded = 0
            block_size = 8192
            last_print_time = time.time()
            
            with open(output_path, 'wb') as f:
                while True:
                    chunk = response.read(block_size)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)
                    
                    # Print progress every 2 seconds
                    current_time = time.time()
                    if current_time - last_print_time >= 2.0:
                        if total_size > 0:
                            pct = (downloaded / total_size) * 100
                            print(f"\r  Progress: {pct:.1f}% ({format_size(downloaded)} / {format_size(total_size)})", end='', flush=True)
                        else:
                            print(f"\r  Downloaded: {format_size(downloaded)}", end='', flush=True)
                        last_print_time = current_time
            
            print(f"\r  Downloaded: {format_size(downloaded)} - Complete!          ")
        
        return True
        
    except urllib.error.URLError as e:
        print(f"  Error downloading: {e}")
        return False
    except Exception as e:
        print(f"  Unexpected error: {e}")
        return False


def main():
    parser = argparse.ArgumentParser(
        description="Download USGS DEM data from National Map CSV export",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument(
        'csv_file',
        help="Path to CSV file exported from USGS National Map Downloader"
    )
    parser.add_argument(
        '--output-dir', '-o',
        default='./dem_data',
        help="Output directory for downloaded files (default: ./dem_data)"
    )
    parser.add_argument(
        '--dry-run', '-n',
        action='store_true',
        help="List files to download without actually downloading"
    )
    parser.add_argument(
        '--skip-existing', '-s',
        action='store_true',
        help="Skip files that already exist in the output directory"
    )
    parser.add_argument(
        '--use-urllib',
        action='store_true',
        help="Use urllib instead of requests (no external dependencies)"
    )
    
    args = parser.parse_args()
    
    # Check if CSV file exists
    csv_path = Path(args.csv_file)
    if not csv_path.exists():
        print(f"Error: CSV file not found: {csv_path}")
        sys.exit(1)
    
    # Parse CSV
    print(f"Reading CSV file: {csv_path}")
    entries = parse_csv(str(csv_path))
    
    if not entries:
        print("No DEM files found in CSV!")
        sys.exit(1)
    
    print(f"Found {len(entries)} DEM files to download:\n")
    
    # Calculate total size if available
    total_size = sum(e['expected_size'] or 0 for e in entries)
    if total_size > 0:
        print(f"Total estimated size: {format_size(total_size)}\n")
    
    # List files
    for i, entry in enumerate(entries, 1):
        size_str = format_size(entry['expected_size']) if entry['expected_size'] else "unknown size"
        print(f"  {i}. {entry['filename']} ({size_str})")
    print()
    
    if args.dry_run:
        print("Dry run mode - no files will be downloaded.")
        return
    
    # Determine download function
    if args.use_urllib or not HAS_REQUESTS:
        if not HAS_REQUESTS:
            print("Note: 'requests' library not found, using urllib fallback")
        download_fn = download_file_urllib
    else:
        download_fn = download_file
    
    # Create output directory
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    print(f"Output directory: {output_dir.absolute()}\n")
    
    # Download files
    success_count = 0
    skip_count = 0
    fail_count = 0
    
    for i, entry in enumerate(entries, 1):
        output_path = output_dir / entry['filename']
        
        print(f"[{i}/{len(entries)}] {entry['filename']}")
        
        # Check if file exists
        if args.skip_existing and output_path.exists():
            file_size = output_path.stat().st_size
            if entry['expected_size'] and file_size == entry['expected_size']:
                print(f"  Skipping (already exists, size matches)")
                skip_count += 1
                continue
            elif entry['expected_size'] is None:
                print(f"  Skipping (already exists, {format_size(file_size)})")
                skip_count += 1
                continue
            else:
                print(f"  File exists but size mismatch, re-downloading...")
        
        # Download
        if download_fn(entry['url'], output_path, entry['expected_size']):
            success_count += 1
        else:
            fail_count += 1
    
    # Summary
    print("\n" + "=" * 50)
    print("Download Summary:")
    print(f"  Successful: {success_count}")
    if skip_count > 0:
        print(f"  Skipped:    {skip_count}")
    if fail_count > 0:
        print(f"  Failed:     {fail_count}")
    print("=" * 50)
    
    if fail_count > 0:
        sys.exit(1)


if __name__ == '__main__':
    main()
