import pymupdf  # PyMuPDF
import re
import argparse
import os

# parser = argparse.ArgumentParser()
# parser.add_argument('-f', '--file')
# args = parser.parse_args()
# 
# 
# doc = pymupdf.open(args.file)
# text = doc[0].get_text()  # First page usually enough
# 
# doi_match = re.search(r'10\.\d{4,}/\S+', text)
# if doi_match:
#     print("DOI found:", doi_match.group())


def extrac_doi(pdf_path):
    doc = pymupdf.open(pdf_path)
    for i in range(min(2, len(doc))):
        text = doc[i].get_text()
        match = re.search(r'10\.\d{4,}/\S+', text)
        if match:
            return match.group().strip()
    return None

def sanitize_doi(doi):
    return doi.replace('/','-').replace(':', '-')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('-f', '--file', help='PDF file')
    args = parser.parse_args()

    if not os.path.isfile(args.file):
        raise ValueError(f"{args.file} doesn't exist!")

    doi = extrac_doi(args.file)

    if not doi:
        raise ValueError(f"No DOI found in {args.file}!")

    print(f"DOI found: {doi}")

    #safe_doi = sanitize_doi(doi)
    #new_path = os.path.join(os.path.dirname(args.file), f"{safe_doi}.pdf")

    #print('new_path:', new_path)

    #os.rename(args.file, new_path)

if __name__ == '__main__':
    main()




